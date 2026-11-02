
bool entry(long param_1,double *param_2,double *param_3)

{
  double *pdVar1;
  double dVar2;
  undefined1 auVar3 [16];
  undefined1 auVar4 [16];
  undefined1 auVar5 [16];
  double dVar6;
  double dVar7;
  double dVar8;
  double dVar9;
  
  pdVar1 = *(double **)(param_1 + 8);
  dVar6 = *param_2;
  dVar8 = param_2[1];
  auVar3._0_8_ = SQRT(dVar8 * dVar8 + dVar6 * dVar6);
  auVar3._8_8_ = 0;
  dVar2 = (double)func_0x000103462b30(auVar3,param_2[2]);
  dVar9 = dVar2 + 6.283185307179586;
  if (0.0 <= dVar2) {
    dVar9 = dVar2;
  }
  dVar2 = pdVar1[8];
  if (dVar2 <= dVar9) {
    auVar5 = NEON_fmov(0xbff0000000000000,8);
  }
  else {
    auVar4._8_8_ = 0;
    auVar4._0_8_ = dVar8;
    func_0x000103462b30(auVar4,dVar6);
    dVar8 = pdVar1[2] + dVar9 * pdVar1[3] + dVar9 * dVar9 * pdVar1[4] +
            dVar9 * dVar9 * dVar9 * pdVar1[5];
    dVar7 = dVar9 * dVar8;
    dVar6 = (double)func_0x0001034627b8();
    dVar8 = dVar8 * dVar7;
    dVar6 = dVar6 * dVar7;
    param_3[1] = dVar6;
    *param_3 = dVar8;
    auVar5._0_8_ = *pdVar1 + pdVar1[0xd] * pdVar1[9] * dVar8;
    auVar5._8_8_ = pdVar1[1] + pdVar1[10] * dVar6;
  }
  param_3[1] = auVar5._8_8_;
  *param_3 = auVar5._0_8_;
  return dVar9 < dVar2;
}

