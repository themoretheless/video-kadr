#!/usr/bin/env python3
"""Read a private native QA PNG; emit only bounded geometric measurements.
Usage: script image.png x_min x_max y_min y_max
"""
import subprocess,math,json,sys,hashlib
from pathlib import Path
path=sys.argv[1];w,h=1024,512
b=subprocess.check_output(['ffmpeg','-v','error','-i',path,'-f','rawvideo','-pix_fmt','gray','pipe:1'])
assert len(b)==w*h
out=[]
for name,(x1,x2,y1,y2) in [('window_frame', tuple(map(int,sys.argv[2:6])))]:
 points=[]
 for y in range(y1+1,y2-1):
  for x in range(x1+1,x2-1):
   gx=b[y*w+x+1]-b[y*w+x-1];gy=b[(y+1)*w+x]-b[(y-1)*w+x]
   if abs(gx)>8 and abs(gx)>2*abs(gy):points.append((x,y,min(abs(gx),80)))
 yr=(y1+y2)/2;best=[]
 for si in range(-120,121):
  slope=si/200;votes={}
  for x,y,weight in points:
   intercept=round((x-slope*(y-yr))/2)*2
   votes[intercept]=votes.get(intercept,0)+weight
  if votes:
   for intercept,score in sorted(votes.items(),key=lambda kv:kv[1],reverse=True)[:8]:best.append((score,slope,intercept))
 selected=[]
 for score,slope,intercept in sorted(best,reverse=True):
  if all(abs(intercept-other['x_at_mid_y'])>14 for other in selected):
   selected.append(dict(score=score,dx_per_dy=slope,up_angle_from_image_y_degrees=math.degrees(math.atan(-slope)),x_at_mid_y=intercept,y_mid=yr))
   if len(selected)==3:break
 out.append(dict(label=name,roi=[x1,x2,y1,y2],edge_points=len(points),vertical_candidates=selected))
print(json.dumps(dict(method="Bounded ROI grayscale-x-gradient weighted Hough; manually selected architectural window ROI, not a general gravity estimator", image_sha256=hashlib.sha256(Path(path).read_bytes()).hexdigest(),image_size=[w,h],parameters=dict(gradient_threshold=8,normal_x_dominance=2,weight_cap=80,slope_step=0.005,slope_range=[-0.6,0.6],intercept_bin_pixels=2,distinct_intercept_pixels=14),observations=out),indent=2))
