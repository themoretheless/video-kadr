#pragma once
#include <stdlib.h>
#include <stdint.h>
#include <errno.h>
#define AVERROR(e) (-(e))
#define FFMAX(a,b) ((a)>(b)?(a):(b))
#define DECLARE_ALIGNED(n,t,v) t __attribute__((aligned(n))) v
#define av_malloc malloc
#define av_calloc calloc
#define av_mallocz(n) calloc(1,n)
#define av_malloc_array(n,s) calloc(n,s)
#define av_free free
#define AVOnce int
#define AV_ONCE_INIT 0
static int ff_thread_once(int *s, void (*f)(void)) {if(!*s){f();*s=1;}return 0;}
