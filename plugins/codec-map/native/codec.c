/* SPDX-License-Identifier: MPL-2.0 */
/* The ABI is compiled against the vendored FFmpeg 8.1 headers. */
#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <libavcodec/avcodec.h>
#include <libavutil/opt.h>
#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
typedef HMODULE Library;
#else
#include <dlfcn.h>
typedef void *Library;
#endif

#define UTIL_FUNCTIONS(X) \
 X(AVFrame *, av_frame_alloc, (void)) \
 X(void, av_frame_free, (AVFrame **)) \
 X(void, av_frame_unref, (AVFrame *)) \
 X(int, av_frame_get_buffer, (AVFrame *, int)) \
 X(int, av_frame_make_writable, (AVFrame *)) \
 X(void, av_frame_remove_side_data, (AVFrame *, enum AVFrameSideDataType)) \
 X(AVFrameSideData *, av_frame_new_side_data, (AVFrame *, enum AVFrameSideDataType, size_t)) \
 X(int, av_dict_set, (AVDictionary **, const char *, const char *, int)) \
 X(void, av_dict_free, (AVDictionary **)) \
 X(int, av_dict_count, (const AVDictionary *)) \
 X(int, av_strerror, (int, char *, size_t)) \
 X(unsigned, avutil_version, (void))
#define CODEC_FUNCTIONS(X) \
 X(const AVCodec *, avcodec_find_encoder_by_name, (const char *)) \
 X(const AVCodec *, avcodec_find_decoder, (enum AVCodecID)) \
 X(AVCodecContext *, avcodec_alloc_context3, (const AVCodec *)) \
 X(void, avcodec_free_context, (AVCodecContext **)) \
 X(int, avcodec_open2, (AVCodecContext *, const AVCodec *, AVDictionary **)) \
 X(int, avcodec_send_frame, (AVCodecContext *, const AVFrame *)) \
 X(int, avcodec_receive_packet, (AVCodecContext *, AVPacket *)) \
 X(int, avcodec_send_packet, (AVCodecContext *, const AVPacket *)) \
 X(int, avcodec_receive_frame, (AVCodecContext *, AVFrame *)) \
 X(AVPacket *, av_packet_alloc, (void)) \
 X(void, av_packet_free, (AVPacket **)) \
 X(void, av_packet_unref, (AVPacket *)) \
 X(uint8_t *, av_packet_get_side_data, (const AVPacket *, enum AVPacketSideDataType, size_t *)) \
 X(unsigned, avcodec_version, (void))

typedef struct CmSession {
    Library util, codec;
#define DECLARE(ret, name, args) ret (*name) args;
    UTIL_FUNCTIONS(DECLARE)
    CODEC_FUNCTIONS(DECLARE)
#undef DECLARE
    AVCodecContext *encoder, *decoder;
    AVFrame *input, *output;
    AVPacket *packet;
    int width, height, kind, use_b, count, target, damaged;
    int64_t next_pts, best_pts;
    uint8_t *drop_flags, *best;
    /* I, P, B, dropped I, dropped B, decoded frames. */
    int stats[6];
} CmSession;

static void close_library(Library lib) {
    if (!lib) return;
#ifdef _WIN32
    FreeLibrary(lib);
#else
    dlclose(lib);
#endif
}

void cm_destroy(CmSession *s) {
    if (!s) return;
    if (s->avcodec_free_context) {
        s->avcodec_free_context(&s->encoder);
        s->avcodec_free_context(&s->decoder);
    }
    if (s->av_frame_free) {
        s->av_frame_free(&s->input);
        s->av_frame_free(&s->output);
    }
    if (s->av_packet_free) s->av_packet_free(&s->packet);
    close_library(s->codec);
    close_library(s->util);
    free(s->drop_flags);
    free(s->best);
    free(s);
}

/* Resolve relative to this plugin, never the process current directory. */
static int default_directory(char *out, size_t capacity) {
#ifdef _WIN32
    HMODULE module = NULL;
    wchar_t path[32768];
    if (!GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
                           GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                           (LPCWSTR)&default_directory, &module)) return 0;
    DWORD count = GetModuleFileNameW(module, path, 32768);
    if (!count || count >= 32768) return 0;
    wchar_t *end = wcsrchr(path, L'\\');
    if (!end) return 0;
    *end = 0;
    return WideCharToMultiByte(CP_UTF8, 0, path, -1, out, (int)capacity,
                               NULL, NULL) > 0;
#else
    Dl_info info;
    if (!dladdr((void *)&default_directory, &info) || !info.dli_fname) return 0;
    if (strlen(info.dli_fname) >= capacity) return 0;
    strcpy(out, info.dli_fname);
    char *end = strrchr(out, '/');
    if (!end) return 0;
    *end = 0;
    return 1;
#endif
}

static Library load_library(const char *directory, const char *name) {
    char path[32768];
    if (snprintf(path, sizeof(path), "%s/%s", directory, name) >= (int)sizeof(path)) return NULL;
#ifdef _WIN32
    wchar_t wide[32768];
    if (!MultiByteToWideChar(CP_UTF8, MB_ERR_INVALID_CHARS, path, -1, wide, 32768)) return NULL;
    for (wchar_t *p = wide; *p; ++p) if (*p == L'/') *p = L'\\';
    return LoadLibraryExW(wide, NULL, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
                                   LOAD_LIBRARY_SEARCH_SYSTEM32);
#else
    return dlopen(path, RTLD_NOW | RTLD_LOCAL);
#endif
}

static void *symbol(Library library, const char *name) {
#ifdef _WIN32
    return (void *)GetProcAddress(library, name);
#else
    return dlsym(library, name);
#endif
}

static int fail(CmSession *s, int code, const char *step, char *error, size_t cap) {
    char detail[128] = "codec error";
    if (s && s->av_strerror) s->av_strerror(code, detail, sizeof(detail));
    snprintf(error, cap, "%s: %s (%d)", step, detail, code);
    return 0;
}

CmSession *cm_create(const char *directory, int width, int height,
                     int time_scale, int time_step, int kind, double quality,
                     int use_b, int count, int target,
                     char *error, size_t cap) {
    char base[32768], path[32768];
    CmSession *s = (CmSession *)calloc(1, sizeof(*s));
    AVDictionary *options = NULL;
    if (!s) { snprintf(error, cap, "Cannot allocate codec session"); return NULL; }
    if (!directory || !*directory) {
        if (!default_directory(base, sizeof(base)) ||
            snprintf(path, sizeof(path), "%s/CodecMap", base) >= (int)sizeof(path)) {
            snprintf(error, cap, "Cannot locate CodecMap runtime directory"); goto cleanup;
        }
        directory = path;
    }
#ifdef _WIN32
    s->util = load_library(directory, "avutil-60.dll");
    if (s->util) s->codec = load_library(directory, "avcodec-62.dll");
#elif defined(__APPLE__)
    s->util = load_library(directory, "libavutil.60.dylib");
    if (s->util) s->codec = load_library(directory, "libavcodec.62.dylib");
#else
    s->util = load_library(directory, "libavutil.so.60");
    if (s->util) s->codec = load_library(directory, "libavcodec.so.62");
#endif
    if (!s->util || !s->codec) {
        snprintf(error, cap, "CodecMap needs FFmpeg 8.1 runtime (avcodec 62 / avutil 60) in %s", directory);
        goto cleanup;
    }
#define LOAD_UTIL(ret, name, args) { void *p = symbol(s->util, #name); if (!p) { snprintf(error, cap, "Missing symbol: %s", #name); goto cleanup; } memcpy(&s->name, &p, sizeof(p)); }
#define LOAD_CODEC(ret, name, args) { void *p = symbol(s->codec, #name); if (!p) { snprintf(error, cap, "Missing symbol: %s", #name); goto cleanup; } memcpy(&s->name, &p, sizeof(p)); }
    UTIL_FUNCTIONS(LOAD_UTIL)
    CODEC_FUNCTIONS(LOAD_CODEC)
#undef LOAD_UTIL
#undef LOAD_CODEC
    if (s->avcodec_version() / 65536 != LIBAVCODEC_VERSION_MAJOR ||
        s->avutil_version() / 65536 != LIBAVUTIL_VERSION_MAJOR ||
        s->avcodec_version() < LIBAVCODEC_VERSION_INT ||
        s->avutil_version() < LIBAVUTIL_VERSION_INT) {
        snprintf(error, cap, "FFmpeg runtime is older than the compiled 8.1 ABI or has a different major version");
        goto cleanup;
    }
    if (width < 2 || height < 2 || width % 2 || height % 2 ||
        width > 16384 || height > 16384 || time_scale <= 0 || time_step <= 0 ||
        kind < 1 || kind > 3 || count < 1 || count > 6003 || target < 0 || target >= count ||
        !(quality >= (kind == 2 ? 0 : 1) && quality <= (kind == 1 ? 51 : kind == 2 ? 63 : 31))) {
        snprintf(error, cap, "Invalid codec settings"); goto cleanup;
    }
    const char *name = kind == 1 ? "libx264" : kind == 2 ? "libvpx-vp9" : "mpeg4";
    const AVCodec *enc = s->avcodec_find_encoder_by_name(name);
    const AVCodec *dec = s->avcodec_find_decoder(kind == 1 ? AV_CODEC_ID_H264 : kind == 2 ? AV_CODEC_ID_VP9 : AV_CODEC_ID_MPEG4);
    if (!enc || !dec) { snprintf(error, cap, "FFmpeg runtime is missing %s encoder/decoder", name); goto cleanup; }
    s->encoder = s->avcodec_alloc_context3(enc);
    s->decoder = s->avcodec_alloc_context3(dec);
    s->input = s->av_frame_alloc();
    s->output = s->av_frame_alloc();
    s->packet = s->av_packet_alloc();
    if (!s->encoder || !s->decoder || !s->input || !s->output || !s->packet) {
        snprintf(error, cap, "Cannot allocate FFmpeg frame/context"); goto cleanup;
    }
    s->width = width; s->height = height; s->kind = kind;
    s->use_b = use_b && kind != 2;
    s->count = count; s->target = target; s->best_pts = -1;
    s->drop_flags = (uint8_t *)calloc(count, 1);
    s->best = (uint8_t *)malloc((size_t)width * height * 3 / 2);
    if (!s->drop_flags || !s->best) { snprintf(error, cap, "Cannot allocate replay state"); goto cleanup; }
    AVCodecContext *ctx = s->encoder;
    ctx->width = width; ctx->height = height;
    ctx->pix_fmt = AV_PIX_FMT_YUV420P;
    ctx->time_base = (AVRational){ time_step, time_scale };
    ctx->framerate = (AVRational){ time_scale, time_step };
    ctx->sample_aspect_ratio = (AVRational){ 1, 1 };
    ctx->gop_size = 6000; ctx->max_b_frames = s->use_b ? 2 : 0;
    ctx->thread_count = 1;
    if (kind == 1) ctx->flags |= AV_CODEC_FLAG_CLOSED_GOP;
    ctx->color_range = AVCOL_RANGE_MPEG;
    ctx->colorspace = AVCOL_SPC_BT709;
    ctx->color_primaries = AVCOL_PRI_BT709;
    ctx->color_trc = AVCOL_TRC_BT709;
    ctx->log_level_offset = 32;
    char quality_text[32];
    snprintf(quality_text, sizeof(quality_text), "%.3f", quality);
#define OPT(key, value) if (s->av_dict_set(&options, key, value, 0) < 0) { snprintf(error, cap, "Cannot set encoder option"); goto cleanup; }
    if (kind == 1) {
        OPT("preset", "veryfast")
        if (!s->use_b) { OPT("tune", "zerolatency") }
        OPT("crf", quality_text)
        OPT("forced-idr", "1")
        OPT("x264-params", "aq-mode=1:aq-strength=1:b-adapt=0:b-pyramid=none:rc-lookahead=0:sync-lookahead=0:mbtree=0:scenecut=0:open-gop=0:intra-refresh=0:ref=1:threads=1:sliced-threads=0")
    } else if (kind == 2) {
        ctx->bit_rate = 0;
        OPT("crf", quality_text)
        OPT("deadline", "realtime")
        OPT("cpu-used", "5")
        OPT("aq-mode", "0")
        OPT("lag-in-frames", "0")
        OPT("auto-alt-ref", "0")
        OPT("error-resilient", "1")
    } else {
        /* Native MPEG-4 has frame-level qscale, but no ROI side-data support. */
        ctx->flags |= AV_CODEC_FLAG_QSCALE;
        ctx->global_quality = (int)(quality * FF_QP2LAMBDA);
        ctx->qmin = 1; ctx->qmax = 31;
    }
#undef OPT
    int result = s->avcodec_open2(ctx, enc, &options);
    if (result < 0) { fail(s, result, name, error, cap); goto cleanup; }
    if (s->av_dict_count(options)) { snprintf(error, cap, "Unrecognized encoder option"); goto cleanup; }
    s->av_dict_free(&options);
    s->decoder->thread_count = 1;
    s->decoder->log_level_offset = 32;
    result = s->avcodec_open2(s->decoder, dec, NULL);
    if (result < 0) { fail(s, result, "Open decoder", error, cap); goto cleanup; }
    s->input->format = AV_PIX_FMT_YUV420P;
    s->input->width = width; s->input->height = height;
    s->input->color_range = AVCOL_RANGE_MPEG;
    s->input->colorspace = AVCOL_SPC_BT709;
    result = s->av_frame_get_buffer(s->input, 32);
    if (result < 0) { fail(s, result, "Allocate YUV frame", error, cap); goto cleanup; }
    return s;
cleanup:
    if (s->av_dict_free) s->av_dict_free(&options);
    cm_destroy(s);
    return NULL;
}

static int receive_frames(CmSession *s, char *error, size_t cap) {
    int result;
    while ((result = s->avcodec_receive_frame(s->decoder, s->output)) >= 0) {
        AVFrame *f = s->output;
        if (f->format != AV_PIX_FMT_YUV420P || f->width != s->width || f->height != s->height ||
            f->pts < 0 || f->pts >= s->count) {
            snprintf(error, cap, "Unexpected decoded timestamp or pixel format"); return 0;
        }
        s->stats[5]++;
        /* A future reference may arrive first. Only publish presentation times <= target. */
        if (f->pts <= s->target && f->pts > s->best_pts) {
            size_t pos = 0;
            for (int p = 0; p < 3; ++p) {
                int w = s->width >> (p != 0), h = s->height >> (p != 0);
                for (int y = 0; y < h; ++y) {
                    memcpy(s->best + pos, f->data[p] + (ptrdiff_t)y * f->linesize[p], w);
                    pos += w;
                }
            }
            s->best_pts = f->pts;
        }
        s->av_frame_unref(f);
    }
    if (result == AVERROR(EAGAIN) || result == AVERROR_EOF ||
        (s->damaged && result == AVERROR_INVALIDDATA)) return 1;
    return fail(s, result, "Receive decoded frame", error, cap);
}

static int receive_packets(CmSession *s, char *error, size_t cap) {
    int result;
    while ((result = s->avcodec_receive_packet(s->encoder, s->packet)) >= 0) {
        AVPacket *p = s->packet;
        if (p->pts < 0 || p->pts >= s->next_pts) {
            snprintf(error, cap, "Unexpected encoded timestamp"); return 0;
        }
        size_t bytes = 0;
        uint8_t *stats = s->av_packet_get_side_data(p, AV_PKT_DATA_QUALITY_STATS, &bytes);
        int type = stats && bytes >= 5 ? stats[4] :
                   (p->flags & AV_PKT_FLAG_KEY) ? AV_PICTURE_TYPE_I : AV_PICTURE_TYPE_P;
        if (s->use_b && (!stats || bytes < 5)) {
            snprintf(error, cap, "Encoder did not report picture types"); return 0;
        }
        if (type < AV_PICTURE_TYPE_I || type > AV_PICTURE_TYPE_B) {
            snprintf(error, cap, "Unsupported encoded picture type"); return 0;
        }
        s->stats[type - AV_PICTURE_TYPE_I]++;
        uint8_t flags = s->drop_flags[p->pts];
        int drop_i = p->pts != 0 && type == AV_PICTURE_TYPE_I && (flags & 1);
        int drop_b = type == AV_PICTURE_TYPE_B && (flags & 2);
        if (drop_i || drop_b) {
            if (drop_i) { s->damaged = 1; s->stats[3]++; }
            if (drop_b) s->stats[4]++;
            s->av_packet_unref(p);
            continue;
        }
        if (type == AV_PICTURE_TYPE_I) s->damaged = 0;
        int sent = s->avcodec_send_packet(s->decoder, p);
        s->av_packet_unref(p);
        /* Missing I references deliberately produce a damaged bitstream. */
        if (sent < 0 && !(s->damaged && sent == AVERROR_INVALIDDATA))
            return fail(s, sent, "Decode packet", error, cap);
        if (!receive_frames(s, error, cap)) return 0;
    }
    if (result == AVERROR(EAGAIN) || result == AVERROR_EOF) return 1;
    return fail(s, result, "Receive encoded packet", error, cap);
}

int cm_push(CmSession *s, const uint8_t *input, size_t input_len,
             const float *offsets, size_t count, int force_i, int drop_flags,
             char *error, size_t cap) {
    if (!s || !input || !offsets) return 0;
    const size_t size = (size_t)s->width * s->height * 3 / 2;
    const int cols = (s->width + 15) / 16, rows = (s->height + 15) / 16;
    if (input_len != size || count != (size_t)cols * rows || s->next_pts >= s->count) {
        snprintf(error, cap, "Invalid YUV or ROI buffer length"); return 0;
    }
    int result = s->av_frame_make_writable(s->input);
    if (result < 0) return fail(s, result, "Writable input", error, cap);
    size_t pos = 0;
    for (int p = 0; p < 3; ++p) {
        int w = s->width >> (p != 0), h = s->height >> (p != 0);
        for (int y = 0; y < h; ++y) {
            memcpy(s->input->data[p] + (ptrdiff_t)y * s->input->linesize[p], input + pos, w);
            pos += w;
        }
    }
    s->av_frame_remove_side_data(s->input, AV_FRAME_DATA_REGIONS_OF_INTEREST);
    if (s->kind != 3) {
    AVFrameSideData *side = s->av_frame_new_side_data(s->input,
        AV_FRAME_DATA_REGIONS_OF_INTEREST, count * sizeof(AVRegionOfInterest));
    if (!side) { snprintf(error, cap, "Cannot allocate ROI map"); return 0; }
    AVRegionOfInterest *roi = (AVRegionOfInterest *)side->data;
    for (int y = 0; y < rows; ++y) for (int x = 0; x < cols; ++x) {
        size_t i = (size_t)y * cols + x;
        if (!(offsets[i] >= -24 && offsets[i] <= 24)) {
            snprintf(error, cap, "Invalid ROI offset"); return 0;
        }
        roi[i].self_size = sizeof(*roi);
        roi[i].left = x * 16; roi[i].right = (x + 1) * 16 < s->width ? (x + 1) * 16 : s->width;
        roi[i].top = y * 16; roi[i].bottom = (y + 1) * 16 < s->height ? (y + 1) * 16 : s->height;
        if (s->kind == 2) {
            /* VP9 has eight segments including neutral. Use seven shared levels. */
            int level = (int)(offsets[i] / 8 + (offsets[i] < 0 ? -0.5f : 0.5f));
            roi[i].qoffset = (AVRational){ level * 8, 63 };
        } else {
            roi[i].qoffset = (AVRational){ (int)(offsets[i] * 1000), 51000 };
        }
    }
    }
    const int64_t pts = s->next_pts++;
    s->drop_flags[pts] = (uint8_t)drop_flags;
    s->input->pts = pts;
    s->input->pict_type = pts == 0 || force_i ? AV_PICTURE_TYPE_I :
                          s->use_b ? AV_PICTURE_TYPE_NONE : AV_PICTURE_TYPE_P;
    s->input->quality = s->encoder->global_quality;
    result = s->avcodec_send_frame(s->encoder, s->input);
    if (result < 0) return fail(s, result, "Encode", error, cap);
    return receive_packets(s, error, cap);
}

int cm_finish(CmSession *s, uint8_t *output, size_t output_len, int *stats,
              char *error, size_t cap) {
    if (!s || !output || output_len != (size_t)s->width * s->height * 3 / 2 ||
        s->next_pts != s->count) {
        snprintf(error, cap, "Incomplete codec replay"); return 0;
    }
    int result = s->avcodec_send_frame(s->encoder, NULL);
    if (result < 0) return fail(s, result, "Flush encoder", error, cap);
    if (!receive_packets(s, error, cap)) return 0;
    result = s->avcodec_send_packet(s->decoder, NULL);
    if (result < 0) return fail(s, result, "Flush decoder", error, cap);
    if (!receive_frames(s, error, cap)) return 0;
    if (s->best_pts < 0) { snprintf(error, cap, "No decoded seed frame"); return 0; }
    memcpy(output, s->best, output_len);
    if (stats) memcpy(stats, s->stats, sizeof(s->stats));
    return 1;
}
