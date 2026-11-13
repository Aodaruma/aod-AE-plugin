"""Rebuild Photo/all contact sheets from existing AE-rendered preview PNGs."""
import argparse
import re
from pathlib import Path
from PIL import Image, ImageDraw, ImageFont

ROOT = Path(__file__).resolve().parent
PATTERN = re.compile(r'\{ name: "([^"]+)", aod: "[^"]+", slug: "([^"]+)", category: "([^"]+)"')
def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--font",type=Path,help="Optional TrueType/OpenType font")
    args=parser.parse_args()
    candidates=[args.font,Path("C:/Windows/Fonts/segoeui.ttf"),Path("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")]
    font_path=next((p for p in candidates if p and p.exists()),None)
    if font_path is None: raise SystemExit("Specify an available font with --font")
    entries=PATTERN.findall((ROOT/"build-catalog.jsx").read_text(encoding="utf-8"))
    if not entries: raise SystemExit("Catalog entries not found")
    for category,filename in [("P","photo.png"),("ALL","all-effects.png")]:
        selected=[e for e in entries if category=="ALL" or e[2]==category]
        columns=min(4,len(selected)); rows=(len(selected)+columns-1)//columns
        image=Image.new("RGB",(600*columns,600*rows),(4,5,8)); draw=ImageDraw.Draw(image)
        for i,(name,slug,_) in enumerate(selected):
            x,y=(i%columns)*600,(i//columns)*600
            draw.rectangle((x+20,y+20,x+579,y+579),fill=(9,11,17))
            source=Image.open(ROOT/"assets"/"previews"/f"{slug}.png").convert("RGBA")
            source=source.resize((420,420),Image.Resampling.LANCZOS)
            image.paste(source,(x+90,y+55),source)
            font=ImageFont.truetype(str(font_path),26 if len(name)>22 else 31)
            draw.text((x+300,y+545),name,font=font,anchor="mm",fill=(240,245,255))
        path=ROOT/"assets"/"overview"/filename
        image.save(path,optimize=True)
        print(f"{path}: {len(selected)} effects, {image.size}")
if __name__=="__main__": main()
