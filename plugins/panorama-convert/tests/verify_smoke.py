"""Check AE PNGs against a continuous spherical direction field and fixed comp bounds."""
from pathlib import Path
import math
from PIL import Image, ImageChops
from make_fixtures import OUT, SIZES, direction, encode
def output_size(fmt,width):
    cols,rows={1:(2,1),2:(3,2),3:(6,1),4:(4,3),5:(1,1)}[fmt]
    n=math.floor(width/cols+.5)
    return cols*n,rows*n
def verify(name,src,dst,width,pan=0,reduced=False):
    im=Image.open(OUT/f"{name}.png").convert("RGBA")
    factor=2 if reduced else 1
    assert im.size==(640//factor,480//factor),(name,im.size)
    w,h=output_size(dst,width or SIZES[src-1][0])
    left,top=(640-w)/2,(480-h)/2
    errors=[]
    for y in range(0,im.height,3):
        for x in range(0,im.width,3):
            u,v=((x+.5)*factor-left)/w,((y+.5)*factor-top)/h
            d=direction(dst,u,v) if 0<=u<1 and 0<=v<1 else None
            if d is not None and pan:
                a=math.radians(pan)
                d=(d[0]*math.cos(a)+d[2]*math.sin(a),d[1],-d[0]*math.sin(a)+d[2]*math.cos(a))
            actual=im.getpixel((x,y))
            if src==5 and d is not None:
                if abs(d[2])<.05: continue
                if d[2]<0: d=None
            if d is None:
                assert actual[3]==0,(name,x,y,"unexpected alpha",actual)
            else:
                # Exclude the fisheye's antialiased silhouette.
                if dst==5 and (2*u-1)**2+(2*v-1)**2>.97: continue
                if actual[3]<250: raise AssertionError((name,x,y,"missing alpha",actual,d))
                expected=encode(d)
                errors.extend(abs(actual[c]-expected[c]) for c in range(3))
    mean=sum(errors)/len(errors)
    assert mean<2.5,(name,"mean error",mean)
    assert max(errors)<15,(name,"max error",max(errors))
    print(f"{name}: PASS mean={mean:.3f} max={max(errors)}")
if __name__=="__main__":
    report=(OUT/"report.txt").read_text()
    assert "PASS: all compositions" in report and "ERROR:" not in report,report
    for src in range(1,6):
        for dst in range(1,6): verify(f"map-{src}-{dst}",src,dst,384)
    for args in [("identity",1,1,0),("smaller",1,1,128),("pan90",1,1,384,90),("half",1,4,384,0,True),("depth-16",1,4,384),("depth-32",1,4,384)]:
        verify(*args)
    image=Image.open(OUT/"identity.png").convert("RGBA").crop((192,176,448,304))
    original=Image.open(OUT/"input-1.png").convert("RGBA")
    assert ImageChops.difference(image,original).getbbox() is None,"identity differs"
    print("PASS: 31 AE renders, identity pixels, and unchanged composition/transform checks.")
