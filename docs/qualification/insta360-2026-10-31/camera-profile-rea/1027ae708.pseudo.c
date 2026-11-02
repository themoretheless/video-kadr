/* REA/Ghidra function-only observation; not original source or product code. */

void entry(double param_1,double param_2,long param_3)

{
  double *pdVar1;
  double dVar2;
  undefined1 auVar3 [16];
  double dVar4;
  
  pdVar1 = *(double **)(param_3 + 8);
  auVar3 = NEON_fmov(0xbff0000000000000,8);
  dVar2 = (param_1 + auVar3._0_8_) / (pdVar1[0x10] + auVar3._0_8_);
  dVar4 = (param_2 + auVar3._8_8_) / (pdVar1[0x11] + auVar3._8_8_);
  pdVar1[1] = dVar4 * pdVar1[1];
  *pdVar1 = dVar2 * *pdVar1;
  dVar2 = dVar2 * pdVar1[0x12];
  dVar4 = dVar4 * pdVar1[0x13];
  auVar3 = NEON_fmov(0x3ff0000000000000,8);
  pdVar1[0x13] = dVar4;
  pdVar1[0x12] = dVar2;
  pdVar1[0x15] = auVar3._8_8_ / dVar4;
  pdVar1[0x14] = auVar3._0_8_ / dVar2;
  pdVar1[0x11] = param_2;
  pdVar1[0x10] = param_1;
  return;
}

