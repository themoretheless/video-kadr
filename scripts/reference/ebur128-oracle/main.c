#include <stdio.h>
#include <stdlib.h>
#include "ebur128.h"
int main(int argc,char**argv){
 if(argc!=4)return 2;
 unsigned rate=atoi(argv[1]), ch=atoi(argv[2]), chunk=atoi(argv[3]);
 FFEBUR128State *s=ff_ebur128_init(ch,rate,3000,FF_EBUR128_MODE_I|FF_EBUR128_MODE_S|FF_EBUR128_MODE_LRA|FF_EBUR128_MODE_SAMPLE_PEAK);
 if(!s)return 3;
 double *buf=malloc(sizeof(double)*ch*chunk);size_t n;
 while((n=fread(buf,sizeof(double)*ch,chunk,stdin)))ff_ebur128_add_frames_double(s,buf,n);
 double i,t,st,l;ff_ebur128_loudness_global(s,&i);ff_ebur128_relative_threshold(s,&t);ff_ebur128_loudness_shortterm(s,&st);ff_ebur128_loudness_range(s,&l);
 printf("%.17g %.17g %.17g %.17g",i,t,st,l);
 for(unsigned c=0;c<ch;c++){double p;ff_ebur128_sample_peak(s,c,&p);printf(" %.17g",p);}puts("");
 ff_ebur128_destroy(&s);free(buf);return 0;
}
