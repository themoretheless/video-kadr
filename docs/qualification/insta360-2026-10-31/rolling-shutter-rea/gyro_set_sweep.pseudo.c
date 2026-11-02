
void entry(double param_1,long param_2)

{
  *(double *)(*(long *)(param_2 + 8) + 0x360) = ABS(param_1) * 0.5 + 0.001;
  return;
}

