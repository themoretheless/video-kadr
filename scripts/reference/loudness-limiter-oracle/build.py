#!/usr/bin/env python3
"""Compile unmodified original peak detector/limiter. Usage: build.py af_loudnorm.c output"""
import hashlib
from pathlib import Path
import subprocess
import sys
import tempfile
if len(sys.argv)!=3: raise SystemExit(__doc__)
raw=Path(sys.argv[1]).read_bytes()
if hashlib.sha256(raw).hexdigest()!='b754ec59c513206fa8cc9930150c0ad2b833bf8ccdd03be63bf0860ac298e93a':
    raise SystemExit('Original af_loudnorm.c hash mismatch')
s=raw.decode()
functions=s[s.index('static void detect_peak'):s.index('static int filter_frame')]
code='''#include <stdio.h>
#include <stdlib.h>
#include <math.h>
enum FrameType {FIRST_FRAME,INNER_FRAME,FINAL_FRAME,LINEAR_MODE};
enum LimiterState {OUT,ATTACK,SUSTAIN,RELEASE};
typedef struct {double *limiter_buf,*prev_smp,target_tp,gain_reduction[2];int limiter_buf_index,limiter_buf_size,peak_index,env_index,env_cnt,attack_length,release_length;enum FrameType frame_type;enum LimiterState limiter_state;} LoudNormContext;
'''+functions+'''
int main(int argc,char **argv) {
 if(argc!=2)return 2;int ch=atoi(argv[1]);if(ch<1||ch>2)return 3;
 LoudNormContext context={0},*s=&context;
 s->limiter_buf_size=40320*ch;s->limiter_buf=calloc(s->limiter_buf_size,sizeof(double));s->prev_smp=calloc(ch,sizeof(double));
 s->target_tp=pow(10.,-1.5/20.);s->attack_length=1920;s->release_length=19200;
 int header[3];
 while(fread(header,sizeof(int),3,stdin)==3) {
 s->limiter_buf_index=header[0];s->frame_type=header[1]?FIRST_FRAME:INNER_FRAME;
 int frames=header[2];
 if(fread(s->limiter_buf,sizeof(double),s->limiter_buf_size,stdin)!=(size_t)s->limiter_buf_size)return 4;
 double *out=calloc(frames*ch,sizeof(double));true_peak_limiter(s,out,frames,ch);
 fwrite(out,sizeof(double),frames*ch,stdout);fwrite(s->limiter_buf,sizeof(double),s->limiter_buf_size,stdout);free(out);
 }
 free(s->limiter_buf);free(s->prev_smp);return 0;
}
'''
output=Path(sys.argv[2]).resolve();output.parent.mkdir(parents=True,exist_ok=True)
with tempfile.TemporaryDirectory(prefix='loudness-limiter-oracle-') as tmp:
    path=Path(tmp)/'oracle.c';path.write_text(code)
    subprocess.run(['cc','-O2','-ffp-contract=off',str(path),'-lm','-o',str(output)],check=True)
print(output)
