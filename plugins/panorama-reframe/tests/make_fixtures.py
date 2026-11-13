"""Generate analytic panorama fixtures; run with an output directory argument."""
from pathlib import Path
import math
import sys
from PIL import Image, ImageDraw

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)

def rgb(d):
    n = math.sqrt(sum(v*v for v in d))
    return tuple(round(127.5*(v/n+1)) for v in d)

def latlong_image(width, height):
    im = Image.new('RGB', (width,height))
    p = im.load()
    for y in range(height):
        lat = ((y+.5)/height-.5)*math.pi
        for x in range(width):
            lon = ((x+.5)/width-.5)*2*math.pi
            p[x,y] = rgb((math.sin(lon)*math.cos(lat), math.sin(lat), math.cos(lon)*math.cos(lat)))
    return im

im = latlong_image(1024,512)
im.save(out/'latlong.png')
# A separate marked grid makes the catalog projection visually legible.
grid=im.copy()
draw=ImageDraw.Draw(grid)
for x in range(0,1024,64): draw.line((x,0,x,511),fill=(245,245,245),width=2)
for y in range(0,512,64): draw.line((0,y,1023,y),fill=(245,245,245),width=2)
for x in range(32,1024,128):
    for y in range(32,512,128): draw.text((x,y),f'{round(x/1024*360-180)} / {round(90-y/512*180)}',fill='white',font_size=18)
grid.save(out/'grid.png')

# Depth deliberately uses a different resolution from the color panorama.
for name,level in [('near',0),('far',255)]:
    Image.new('RGB',(256,128),(level,level,level)).save(out/f'depth-{name}.png')
Image.new('RGBA',(256,128),(0,0,0,0)).save(out/'depth-transparent.png')
depth=Image.new('RGB',(256,128),'white')
ImageDraw.Draw(depth).rectangle((0,0,255,63),fill='black')
depth.save(out/'depth-split.png')

layouts=[('horizontal',6,1,[(i,0) for i in range(6)]),('vertical',1,6,[(0,i) for i in range(6)]),('grid',3,2,[(i%3,i//3) for i in range(6)]),('cross',4,3,[(2,1),(0,1),(1,2),(1,0),(1,1),(3,1)])]
for name,cols,rows,cells in layouts:
    # Match the output aspect without AE resampling the reference's pole opacity.
    latlong_image(cols*128,rows*128).save(out/f'{name}-reference.png')
    for eac in (False,True):
        im=Image.new('RGB',(cols*128,rows*128))
        depth=Image.new('RGB',im.size)
        dp=depth.load()
        p=im.load()
        for f,(cx,cy) in enumerate(cells):
            for y in range(128):
                v=2*(y+.5)/128-1
                if eac: v=math.tan(v*math.pi/4)
                for x in range(128):
                    u=2*(x+.5)/128-1
                    if eac: u=math.tan(u*math.pi/4)
                    d=[(1,v,-u),(-1,v,u),(u,1,-v),(u,-1,v),(u,v,1),(-u,v,-1)][f]
                    p[cx*128+x,cy*128+y]=rgb(d)
                    level=0 if d[1]<0 else 255
                    dp[cx*128+x,cy*128+y]=(level,level,level)
        im.save(out/f'{name}-{"eac" if eac else "cube"}.png')
        depth.save(out/f'depth-{name}-{"eac" if eac else "cube"}.png')
