"""Create independent analytic direction-color fixtures for AE integration tests."""
from pathlib import Path
import math
from PIL import Image
ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "target" / "panorama-smoke"
OUT.mkdir(parents=True, exist_ok=True)
SIZES = [(256,128),(192,128),(384,64),(256,192),(128,128)]
CELLS = [(2,1),(0,1),(1,0),(1,2),(1,1),(3,1)]
def direction(fmt, u, v):
    if fmt == 1:
        lon,lat=(u-.5)*math.tau,(.5-v)*math.pi
        return math.sin(lon)*math.cos(lat),math.sin(lat),math.cos(lon)*math.cos(lat)
    if fmt == 5:
        x,y=2*u-1,1-2*v
        r=math.hypot(x,y)
        if r>1: return None
        if r<1e-12: return 0,0,1
        a=r*math.pi/2
        return x/r*math.sin(a),y/r*math.sin(a),math.cos(a)
    cols,rows={2:(3,2),3:(6,1),4:(4,3)}[fmt]
    cx,cy=int(u*cols),int(v*rows)
    if fmt==4:
        if (cx,cy) not in CELLS: return None
        face=CELLS.index((cx,cy))
    else: face=cy*cols+cx
    a,b=2*(u*cols-cx)-1,2*(v*rows-cy)-1
    vec=[(1,-b,-a),(-1,-b,a),(a,1,b),(a,-1,-b),(a,-b,1),(-a,-b,-1)][face]
    n=math.sqrt(sum(c*c for c in vec))
    return tuple(c/n for c in vec)
def encode(d):
    return (0,0,0,0) if d is None else tuple(round((c*.5+.5)*255) for c in d)+(255,)
if __name__=="__main__":
    for fmt,(w,h) in enumerate(SIZES,1):
        im=Image.new("RGBA",(w,h))
        im.putdata([encode(direction(fmt,(x+.5)/w,(y+.5)/h)) for y in range(h) for x in range(w)])
        im.save(OUT/f"input-{fmt}.png")
    print(OUT)
