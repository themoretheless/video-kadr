/* REA/Ghidra function-only observation; not original source or product code. */

undefined8 entry(double *param_1,double *param_2,double *param_3)

{
  double dVar1;
  undefined1 auVar2 [16];
  double dVar3;
  double dVar4;
  double dVar5;
  double dVar6;
  double dVar7;
  double dVar8;
  
  dVar1 = *param_2;
  dVar3 = param_2[1];
  dVar5 = dVar3 * dVar3 + dVar1 * dVar1;
  if (dVar5 <= 0.0) {
    dVar1 = *param_1;
    param_3[1] = param_1[1];
    *param_3 = dVar1;
    return 1;
  }
  dVar4 = param_2[2];
  dVar5 = SQRT(dVar5 + dVar4 * dVar4);
  if (param_1[0x17] + -0.01 <= dVar4 / dVar5) {
    dVar5 = 1.0 / (dVar4 + dVar5 * param_1[0x16]);
    dVar1 = dVar1 * dVar5;
    dVar3 = dVar3 * dVar5;
    param_3[1] = dVar3;
    *param_3 = dVar1;
    dVar7 = dVar1 * dVar1 + dVar3 * dVar3;
    dVar8 = dVar7 * dVar7;
    dVar5 = dVar7 * dVar7 * dVar8;
    dVar6 = dVar7 * param_1[2] + 1.0 + dVar8 * param_1[3] + dVar7 * dVar8 * param_1[4] +
            dVar5 * param_1[5] + dVar7 * dVar5 * param_1[6];
    dVar5 = param_1[8] + param_1[10] * dVar7;
    dVar5 = (dVar7 + dVar1 * dVar1 * 2.0) * (param_1[7] + param_1[9] * dVar7) + dVar6 * dVar1 +
            dVar1 * dVar3 * (dVar5 + dVar5) + dVar7 * param_1[0xb] + dVar8 * param_1[0xd];
    *param_3 = dVar5;
    dVar4 = param_1[7] + param_1[9] * dVar7;
    dVar1 = (dVar7 + dVar3 * dVar3 * 2.0) * (param_1[8] + param_1[10] * dVar7) + dVar6 * dVar3 +
            dVar1 * dVar3 * (dVar4 + dVar4) + dVar7 * param_1[0xc] + dVar8 * param_1[0xe];
    param_3[1] = dVar1;
    dVar4 = param_1[0x12];
    dVar6 = param_1[0x18];
    dVar3 = *param_1;
    param_3[1] = param_1[1] + dVar1 * param_1[0x13];
    *param_3 = dVar3 + dVar5 * dVar4 * dVar6;
    return 1;
  }
  auVar2 = NEON_fmov(0xbff0000000000000,8);
  param_3[1] = auVar2._8_8_;
  *param_3 = auVar2._0_8_;
  return 0;
}

