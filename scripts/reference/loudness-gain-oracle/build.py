#!/usr/bin/env python3
"""Compile original Loudnorm Gaussian/history expressions, independently of Rust.
Usage: build.py /path/to/af_loudnorm.c /output/gain-oracle
"""
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
if len(sys.argv)!=3:
    raise SystemExit(__doc__)
source=Path(sys.argv[1]).read_bytes()
if hashlib.sha256(source).hexdigest()!='b754ec59c513206fa8cc9930150c0ad2b833bf8ccdd03be63bf0860ac298e93a':
    raise SystemExit('Original af_loudnorm.c hash mismatch')
s=source.decode()
filters=s[s.index('static void init_gaussian_filter'):s.index('static void detect_peak')]
update=s[s.index('        if (s->above_threshold == 0)'):s.index('        s->prev_nb_samples = in->nb_samples;')]
update=update.replace('ff_ebur128_loudness_shortterm(s->r128_out, &shortterm_out);','shortterm_out = output_shortterm;')
code='''#include <stdio.h>
#include <math.h>
typedef struct { double weights[21],delta[30],prev_delta,target_i,target_lra,measured_thresh; int index,above_threshold; } LoudNormContext;
'''+filters+'''
int main(void) {
 LoudNormContext context={0}, *s=&context;
 double initial,global,shortterm,relative_threshold,output_shortterm,env_global,env_shortterm;
 if(scanf("%lf %lf",&s->target_i,&initial)!=2)return 2;
 s->target_lra=11.;s->measured_thresh=-70.;s->above_threshold=initial>=-70.;
 init_gaussian_filter(s);
 double gain=pow(10.,(initial<=-70.?0.:s->target_i-initial)/20.);
 for(int i=0;i<30;i++)s->delta[i]=gain;s->prev_delta=gain;
 while(scanf("%lf %lf %lf %lf",&global,&shortterm,&relative_threshold,&output_shortterm)==4) {
 printf("%.17g %.17g\\n",gaussian_filter(s,(s->index+10)%30),gaussian_filter(s,(s->index+11)%30));
'''+update+'''
 }
 return 0;
}
'''
output=Path(sys.argv[2]).resolve();output.parent.mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory(prefix='loudnorm-gain-oracle-') as tmp:
    path=Path(tmp)/'oracle.c';path.write_text(code)
    subprocess.run(['cc','-O2','-ffp-contract=off',str(path),'-lm','-o',str(output)],check=True)
print(output)
