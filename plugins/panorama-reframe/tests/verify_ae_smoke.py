"""Verify actual AE output; Pillow required. Usage: verify_ae_smoke.py .task/ae"""
from pathlib import Path
import math
import sys
from PIL import Image, ImageChops, ImageStat

root=Path(sys.argv[1])
def image(name):
    with Image.open(root/f'{name}.png') as im:
        return im.convert('RGBA')
def equal(a,b,tolerance=0):
    diff=ImageChops.difference(image(a),image(b))
    assert max(high for _,high in diff.getextrema())<=tolerance, f'{a} differs from {b}: {diff.getextrema()}'
    print(f'PASS {a} / {b}')

log=(root/'result.txt').read_text()
assert 'PASS script completed' in log and 'FAIL' not in log,log
assert 'AE 2025 version 25.' in log,log
# The precomposed input and projection each round 8-bit samples once.
for a,b,tolerance in [('perspective-8','perspective-16',2),('yaw90','quaternion90',0),('perspective-32','camera-default',0),('yaw90','camera-parent90',0)]:
    equal(a,b,tolerance)
for layout in range(1,5):
    for projection in (2,3): equal(f'cube-reference-{layout}',f'cube-{layout}-{projection}',1)

# An independent analytic ray oracle catches sign, FOV and pixel-center errors.
im=image('perspective-8')
for x,y in [(0,0),(319,179),(160,90),(51,133),(279,36)]:
    d=((2*(x+.5)/320-1), (2*(y+.5)/180-1)/(320/180), 1)
    n=math.sqrt(sum(v*v for v in d))
    expected=tuple(round(127.5*(v/n+1)) for v in d)+(255,)
    actual=im.getpixel((x,y))
    assert max(abs(a-b) for a,b in zip(actual,expected))<=2,(actual,expected)
print('PASS independent perspective ray samples')

half=image('half');assert half.size==(160,90)
full=image('perspective-32').resize(half.size,Image.Resampling.BOX)
assert max(v[1] for v in ImageChops.difference(half,full).getextrema())<=1
for name in ('fisheye','orthographic'):
    im=image(name); assert im.getpixel((0,0))[3]==0 and im.getpixel((160,90))[3]==255
print('PASS half resolution and projection masks')

# On some AE 25 installations saveFrameToPng scales float RGB for SDR export.
# Assert the actual float sample separately rather than compensating the plugin.
sample=next(line.split('float sample ')[1] for line in log.splitlines() if line.startswith('float sample '))
rgba=list(map(float,sample.split(',')))
assert abs(rgba[0]-.5)<.01 and abs(rgba[1]-.5)<.01 and abs(rgba[2]-1)<.001 and rgba[3]==1,rgba
assert image('raw32').getpixel((512,256))==image('perspective-32').getpixel((160,90))
print('PASS actual 32bpc values and native export parity')
assert image('position').getpixel((160,90))[0]>image('perspective-32').getpixel((160,90))[0]
assert ImageStat.Stat(ImageChops.difference(image('camera-position'),image('camera-default'))).sum[0]>0
assert image('isometric').getpixel((160,90))[1]<image('isometric').getpixel((160,90))[0]
for f in range(1,8): assert image(f'filter-{f}').getchannel('A').getextrema()==(255,255)
print('PASS position, isometric and all interpolation filters')

for axis in 'XYZ':
    for sign in ('plus','minus'):
        assert image(f'environment-{axis}-{sign}').getchannel('A').getextrema()==(255,255)
# The center ray is mapped by a half-angle contraction exp(-asinh(0.5)).
center=image('environment-half-depth').getpixel((160,90))
assert abs(center[0]-round(127.5*(1+0.5/math.sqrt(1.25))))<=2,center
assert abs(center[2]-round(127.5*(1+1/math.sqrt(1.25))))<=2,center
print('PASS environment warp on all axes and beyond depth')

for bits in (8,16,32):
    equal(f'depth-near-{bits}',f'depth-near-reference-{bits}',1)
    for name in ('far','transparent','disabled'):
        equal(f'depth-{name}-{bits}',f'depth-far-reference-{bits}',1)
    split=image(f'depth-split-{bits}')
    for x,y,region in [(160,30,'near'),(160,150,'far'),(40,45,'near'),(279,130,'far')]:
        actual=split.getpixel((x,y)); expected=image(f'depth-{region}-reference-{bits}').getpixel((x,y))
        assert max(abs(a-b) for a,b in zip(actual,expected))<=1,(bits,actual,expected)
equal('depth-inverted','depth-far-reference-8',1)
equal('depth-zero','perspective-8',0)
equal('depth-animated-near','depth-near-reference-8',1)
equal('depth-animated-far','depth-far-reference-8',1)
equal('depth-removed-after-animation','depth-far-reference-8',0)
assert image('depth-near-8').getpixel((160,30))[0]>image('depth-far-8').getpixel((160,30))[0]+30
assert image('depth-extreme').getchannel('A').getextrema()==(255,255)
half=image('depth-half');full=image('depth-split-8').resize(half.size,Image.Resampling.BOX)
# Ignore a narrow band at the deliberate discontinuity in the depth map.
for region in [(0,0,160,40),(0,50,160,90)]:
    assert max(v[1] for v in ImageChops.difference(half.crop(region),full.crop(region)).getextrema())<=2
for layout in range(1,5):
    for projection in (2,3):
        im=image(f'depth-cube-{layout}-{projection}');ref=image(f'depth-cube-reference-{layout}')
        for region in [(0,0,320,75),(0,105,320,180)]:
            assert max(v[1] for v in ImageChops.difference(im.crop(region),ref.crop(region)).getextrema())<=2
print('PASS spatial depth, inverse convention, animation, no-map fallback, half resolution and all cube/EAC layouts')

for bits in (8,16,32):
    for proj in (2,3,4):
        for scale in (50,100,150):
            im=image(f'sphere-{proj}-{scale}-{bits}')
            radius=(90 if proj==2 else 80)*scale/100
            for x,y in [(160,90),(0,0),(160+int(radius*.8),90),(160+int(radius*1.05),90)]:
                if x>=320:continue
                inside=math.hypot(x+.5-160,y+.5-90)<radius
                assert im.getpixel((x,y))[3]==(255 if inside else 0),(bits,proj,scale,x,y)
            box=im.getchannel('A').getbbox()
            assert abs((box[2]-box[0])-2*radius)<=2,(bits,proj,scale,box)
            if bits!=32:
                # The scale changes only image coordinates, not the sampled field of view.
                x,y=160+int(radius*.45),90-int(radius*.2)
                dx,dy=(x+.5-160)/radius,(y+.5-90)/radius
                if proj==2:
                    rho=math.hypot(dx,dy);theta=rho*math.pi/2
                    d=[dx/rho*math.sin(theta),dy/rho*math.sin(theta),math.cos(theta)]
                else:
                    d=[dx,dy,math.sqrt(1-dx*dx-dy*dy)]
                    if proj==4:
                        ax=math.radians(35.26438968);ay=math.pi/4
                        d=[d[0],d[1]*math.cos(ax)-d[2]*math.sin(ax),d[1]*math.sin(ax)+d[2]*math.cos(ax)]
                        d=[d[0]*math.cos(ay)+d[2]*math.sin(ay),d[1],-d[0]*math.sin(ay)+d[2]*math.cos(ay)]
                expected=tuple(round(127.5*(v+1)) for v in d)+(255,)
                actual=im.getpixel((x,y))
                assert max(abs(a-b) for a,b in zip(actual,expected))<=2,(bits,proj,scale,actual,expected)
equal('sphere-animated-50','sphere-2-50-8',0)
equal('sphere-animated-150','sphere-2-150-8',0)
equal('sphere-perspective','perspective-8',0)
assert image('sphere-half').getchannel('A').getbbox()==(58,23,102,67)
box=image('sphere-par2').getchannel('A').getbbox()
assert abs((box[2]-box[0])*2-(box[3]-box[1]))<=2,box
print('PASS sphere scale masks, unchanged view angles, animation, half resolution and pixel aspect')

assert 'PASS native input, preceding effects and masks' in log
for bits in (8,16,32):
    equal(f'pipeline-before-invert-{bits}',f'pipeline-after-invert-{bits}',2)
    mask=image(f'pipeline-mask-{bits}')
    assert mask.getpixel((40,90))[3]==0 and mask.getpixel((280,90))[3]==255
    baseline=image(f'perspective-{bits}')
    assert max(ImageStat.Stat(ImageChops.difference(image(f'pipeline-before-invert-{bits}'),baseline)).sum[:3])>1000
assert image('direct-footage').size==(1024,512)
assert image('direct-footage').getchannel('A').getextrema()==(255,255)
assert image('direct-cropped').size==(512,512)
crop_diff=ImageChops.difference(image('direct-cropped'),image('direct-footage').crop((256,0,768,512)))
assert max(high for _,high in crop_diff.getextrema())==0,crop_diff.getextrema()
print('PASS direct native footage, cropped composition, preceding effects and cropped input masks')
