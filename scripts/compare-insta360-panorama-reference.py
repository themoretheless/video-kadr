#!/usr/bin/env python3
"""Diagnostic fixed-SO3 comparison of privately supplied equirectangular images."""
from pathlib import Path
import numpy as np
from PIL import Image
from scipy.ndimage import map_coordinates, gaussian_filter
from scipy.spatial.transform import Rotation
from scipy.optimize import differential_evolution, minimize
from types import SimpleNamespace
import json
import argparse, hashlib
parser=argparse.ArgumentParser(description="Single-frame diagnostic alignment; not a vendor parity certificate. Requires numpy, scipy and pillow.")
parser.add_argument('--reference',required=True,type=Path)
parser.add_argument('--feather',required=True,type=Path)
parser.add_argument('--flow',required=True,type=Path)
parser.add_argument('--output-dir',required=True,type=Path)
parser.add_argument('--rotation-from',type=Path,help='Reuse a prior report rotation exactly; no refitting')
parser.add_argument('--previous-flow',type=Path,help='Also measure the preceding optical renderer')
parser.add_argument('--rect',nargs=5,action='append',default=[],metavar=('NAME','X','Y','WIDTH','HEIGHT'))
args=parser.parse_args()
q=args.output_dir
q.mkdir(parents=True,exist_ok=True)
if any((q/name).exists() for name in ['native-feather-aligned.png','native-flow-aligned.png','native-previousFlow-aligned.png','reference-measurement.json']):
    raise SystemExit('Output files already exist; choose a new private directory')
def load(p): return np.asarray(Image.open(p).convert('RGB'),dtype=float)/255
ref=load(args.reference)
base=load(args.feather);flow=load(args.flow)
h,w=ref.shape[:2]
if base.shape != ref.shape or flow.shape != ref.shape: raise SystemExit('Inputs must have identical RGB dimensions')
if w != 2*h or h < 48: raise SystemExit('Inputs must be equirectangular 2:1 images at least 96x48')
if min(ref.std(),base.std(),flow.std()) < 1e-5: raise SystemExit('Flat or black input cannot be aligned')
y,x=np.mgrid[12:h-12:5,0:w:5];lon=(x.ravel()+.5)/w*2*np.pi-np.pi;lat=np.pi/2-(y.ravel()+.5)/h*np.pi
rays=np.stack([np.sin(lon)*np.cos(lat),np.sin(lat),np.cos(lon)*np.cos(lat)],axis=1)
target=gaussian_filter(ref.mean(2),1)[y,x].ravel();target=(target-target.mean())/target.std()
a=gaussian_filter(base.mean(2),1)
def coords(r,shape):
 lon=np.arctan2(r[:,0],r[:,2]);lat=np.arcsin(np.clip(r[:,1],-1,1));return [np.clip((.5-lat/np.pi)*shape[0]-.5,0,shape[0]-1),((lon/(2*np.pi)+.5)*shape[1]-.5)%shape[1]]
def objective(e):
 v=map_coordinates(a,coords(Rotation.from_euler('xyz',e).apply(rays),a.shape),order=1,mode='grid-wrap');v=(v-v.mean())/v.std();return np.mean(np.minimum((v-target)**2,4))
if args.rotation_from:
 e=np.asarray(json.loads(args.rotation_from.read_text())['rotationEulerXYZRadians'],dtype=float)
 if e.shape!=(3,) or not np.isfinite(e).all(): raise SystemExit('Invalid fixed rotation')
 r=SimpleNamespace(x=e,fun=objective(e))
else:
 global_fit=differential_evolution(objective,[(-np.pi,np.pi)]*3,popsize=12,maxiter=100,seed=10,tol=1e-6,workers=1)
 # A global stochastic search can miss the narrow identity basin even when
 # native and reference body coordinates nearly coincide. Compare deterministic
 # identity/local candidates using the SAME feather-only objective; flow never
 # participates in registration or candidate selection.
 candidates=[global_fit,SimpleNamespace(x=np.zeros(3),fun=objective(np.zeros(3)))]
 for initial in [np.zeros(3),global_fit.x]:
  candidate=minimize(objective,initial,method='Powell',options={'maxiter':100,'xtol':1e-7,'ftol':1e-9})
  if np.isfinite(candidate.fun) and np.isfinite(candidate.x).all(): candidates.append(candidate)
 r=min(candidates,key=lambda candidate:candidate.fun)
print('rotation',r.x,'loss',r.fun,flush=True)
yy,xx=np.mgrid[:h,:w];ll=(xx.ravel()+.5)/w*2*np.pi-np.pi;tt=np.pi/2-(yy.ravel()+.5)/h*np.pi;rr=np.stack([np.sin(ll)*np.cos(tt),np.sin(tt),np.cos(ll)*np.cos(tt)],axis=1);co=coords(Rotation.from_euler('xyz',r.x).apply(rr),a.shape)
metrics={}
warped_results={}
variants=[('feather',base),('flow',flow)]
if args.previous_flow:
 previous=load(args.previous_flow)
 if previous.shape!=ref.shape: raise SystemExit('Previous flow dimensions differ')
 variants.append(('previousFlow',previous))
for name,im in variants:
 warped=np.stack([map_coordinates(im[:,:,k],co,order=1,mode='grid-wrap') for k in range(3)],1).reshape(h,w,3);Image.fromarray(np.uint8(np.clip(warped*255,0,255))).save(q/f'native-{name}-aligned.png')
 warped_results[name]=warped
 diff=warped-ref;weights=np.cos((np.arange(h)+.5)/h*np.pi-np.pi/2)[:,None,None];mse=np.sum(diff**2*weights)/(weights.sum()*w*3)
 metrics[name]={'sphereWeightedMse':float(mse),'psnr':float(-10*np.log10(mse))}
out={'inputSha256': {name:hashlib.sha256(path.read_bytes()).hexdigest() for name,path in [('reference',args.reference),('feather',args.feather),('flow',args.flow)]},'fittedOn':'feather frame0 only; downsampled-luma SO3, deterministic identity/local and global candidates; identical rotation for every variant','rotationEulerXYZRadians':r.x.tolist(),'objective':float(r.fun),'metrics':metrics,'limitation':'Different renderer calibration, resampling, chromatic calibration, seam modes, and time-selection may contribute. Single frame is not full vendor parity.'}
if args.rotation_from:
 out['rotationSourceSha256']=hashlib.sha256(args.rotation_from.read_bytes()).hexdigest()
 out['fittedOn']='Exact rotation reused from prior report; no refitting of either current variant'
if args.previous_flow: out['inputSha256']['previousFlow']=hashlib.sha256(args.previous_flow.read_bytes()).hexdigest()
out['fixedRectangles']={}
for name,sx,sy,sw,sh in args.rect:
 x0,y0,rw,rh=map(int,[sx,sy,sw,sh])
 if not (0<=x0<x0+rw<=w and 0<=y0<y0+rh<=h): raise SystemExit('Invalid evaluation rectangle')
 out['fixedRectangles'][name]=[x0,y0,rw,rh]
 for label,warped in warped_results.items():
  difference=warped[y0:y0+rh,x0:x0+rw]-ref[y0:y0+rh,x0:x0+rw]
  weight=weights[y0:y0+rh]
  mse=np.sum(difference**2*weight)/(weight.sum()*rw*3)
  metrics[label].setdefault('fixedRectangles',{})[name]={'mse':float(mse),'psnr':float(-10*np.log10(mse))}
mask0=np.max(abs(base-flow),axis=2)>1/255
mask=map_coordinates(mask0.astype(float),co,order=1,mode='grid-wrap').reshape(h,w)>.5
for name in ['feather','flow']:
 warped=warped_results[name];diff=warped-ref;ww=weights[:,:,0]*mask
 if ww.sum()>0:
  mse=np.sum(np.mean(diff**2,2)*ww)/ww.sum();out['metrics'][name]['changedRegionMse']=float(mse);out['metrics'][name]['changedRegionPsnr']=float(-10*np.log10(mse))
out['changedRegionPixels']=int(mask.sum());out['changedRegionDefinition']='Same fixed SO3 for both images. Native RGB feather-versus-flow difference >1/255 before rotation; rotate and threshold interpolated mask >0.5.'
(q/'reference-measurement.json').write_text(json.dumps(out,indent=2));print(json.dumps(out,indent=2))
