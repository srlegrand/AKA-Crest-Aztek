import numpy as np, subprocess, io
from PIL import Image, ImageDraw, ImageFont
from scipy.ndimage import gaussian_filter
W,H=1200,700
rng=np.random.default_rng(7)
def svgmask(svg,w,h):
    png=subprocess.run(['rsvg-convert','-w',str(w),'-h',str(h)],input=svg.encode(),capture_output=True).stdout
    return np.asarray(Image.open(io.BytesIO(png)).convert('L'),float)/255
# stone
h=np.zeros((H,W))
for s,a in [(60,1.0),(20,.5),(6,.25),(2,.12),(.7,.08)]:
    n=gaussian_filter(rng.standard_normal((H,W)),s); h+=a*n/n.std()
h*=0.012
# step-fret relief: two bands, top and bottom, of square spirals
fret=Image.new('L',(W,H),0); d=ImageDraw.Draw(fret)
def spiral(x,y,s,flip):
    u=s/6; pts=[(0,6),(0,0),(6,0),(6,4),(2,4),(2,2),(4,2)]
    if flip: pts=[(6-a,6-b) for a,b in pts]
    d.line([(x+a*u,y+b*u) for a,b in pts],fill=255,width=int(u*0.9),joint='curve')
s=90
for row,y in enumerate([28,H-28-s]):
    for i,x in enumerate(range(560,W+s,s+18)):
        spiral(x,y,s,(i+row)%2)
fm=gaussian_filter(np.asarray(fret,float)/255,2.2)
h+=0.9*fm
# big carved mark from the icon
icon='''<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512"><g fill="none" stroke="#fff" stroke-width="34" stroke-linecap="square" stroke-linejoin="miter"><path d="M90 450 L210 62 L330 450"/><path d="M135 310 H422 V400 H332 V355 H377"/></g></svg>'''
m=np.zeros((H,W)); ms=430; ox,oy=730,135
m[oy:oy+ms,ox:ox+ms]=svgmask(icon,ms,ms)
carve=gaussian_filter(m,3.0)
h-=1.6*carve
# frame lines (raised border round the mark)
fr=Image.new('L',(W,H),0); d=ImageDraw.Draw(fr)
d.rectangle([ox-30,oy-15,ox+ms+5,oy+ms+15],outline=255,width=10)
h+=0.7*gaussian_filter(np.asarray(fr,float)/255,2)
# shading
gy,gx=np.gradient(h*40)
nz=1/np.sqrt(gx**2+gy**2+1); nx=-gx*nz; ny=-gy*nz
L=np.array([-0.62,-0.55,0.56]); L/=np.linalg.norm(L)
lam=np.clip(nx*L[0]+ny*L[1]+nz*L[2],0,1)
ao=np.clip(1+3*(h-gaussian_filter(h,8)),0.25,1.2)
img=lam**1.6*ao
img*=0.85+0.15*gaussian_filter(rng.random((H,W)),0.6)/0.5
# dramatic light pool on the mark, fade to black on the left
yy,xx=np.mgrid[0:H,0:W]
pool=np.exp(-(((xx-930)/430)**2+((yy-330)/330)**2))
fade=np.clip((xx-430)/380,0,1)**1.7
img=img*(0.15+1.1*pool)*fade
img=np.clip(img*1.35,0,1)**1.35
out=Image.fromarray((img*255).astype(np.uint8)).convert('RGB')
# logo top-left
logo=svgmask(open('/home/claude/aztek/app/icons/aztek-logo.svg').read(),460,233)
lg=Image.fromarray((logo*255).astype(np.uint8))
out.paste(Image.new('RGB',lg.size,(255,255,255)),(40,50),lg)
out.save('splash_raw.png')
