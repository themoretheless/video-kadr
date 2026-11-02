
undefined8 * entry(undefined8 *param_1,long *param_2)

{
  ulong uVar1;
  undefined8 uVar2;
  undefined8 *puVar3;
  ulong uVar4;
  ulong uVar5;
  undefined8 *puVar6;
  ulong uVar7;
  long lVar8;
  long lVar9;
  ulong uVar10;
  long lVar11;
  undefined8 uVar12;
  undefined8 uVar13;
  double dVar14;
  double dVar15;
  undefined1 auVar16 [16];
  undefined8 uVar17;
  undefined8 uVar18;
  undefined8 uVar19;
  undefined8 uVar20;
  undefined8 *local_40;
  ulong local_38;
  
  uVar12 = *(undefined8 *)*param_2;
  param_1[1] = ((undefined8 *)*param_2)[1];
  *param_1 = uVar12;
  lVar11 = *param_2;
  local_40 = (undefined8 *)0x0;
  local_38 = 0;
  func_0x0001013bb6c0(&local_40,4,1);
  if (local_38 == 4) {
    uVar4 = 4;
    uVar7 = 4;
LAB_1027ad350:
    lVar8 = 0;
    lVar9 = 0;
    do {
      puVar3 = (undefined8 *)(lVar11 + 0x10 + lVar8);
      uVar12 = *puVar3;
      ((undefined8 *)((long)local_40 + lVar8))[1] = puVar3[1];
      *(undefined8 *)((long)local_40 + lVar8) = uVar12;
      lVar9 = lVar9 + 2;
      lVar8 = lVar8 + 0x10;
    } while (lVar9 < (long)uVar4);
  }
  else {
    func_0x0001013bb6c0(&local_40,4,1);
    uVar4 = local_38;
    if ((long)local_38 < 0) {
      uVar4 = local_38 + 1;
    }
    uVar4 = uVar4 & 0xfffffffffffffffe;
    uVar7 = local_38;
    if (1 < (long)local_38) goto LAB_1027ad350;
  }
  uVar1 = uVar7 - uVar4;
  if (uVar1 != 0 && (long)uVar4 <= (long)uVar7) {
    uVar5 = uVar4;
    if (9 < uVar1) {
      puVar3 = (undefined8 *)(uVar4 * 8 + lVar11);
      if (0x3f < (long)local_40 + (uVar4 * 8 - (long)puVar3) + -0x10) {
        uVar10 = uVar1 & 0xfffffffffffffff8;
        uVar5 = uVar4 + uVar10;
        puVar6 = local_40 + uVar4 + 4;
        uVar4 = uVar10;
        do {
          uVar13 = puVar3[2];
          uVar12 = puVar3[4];
          uVar2 = puVar3[5];
          uVar18 = puVar3[7];
          uVar17 = puVar3[6];
          uVar20 = puVar3[9];
          uVar19 = puVar3[8];
          puVar6[-3] = puVar3[3];
          puVar6[-4] = uVar13;
          puVar6[-1] = uVar2;
          puVar6[-2] = uVar12;
          puVar6[1] = uVar18;
          *puVar6 = uVar17;
          puVar6[3] = uVar20;
          puVar6[2] = uVar19;
          uVar4 = uVar4 - 8;
          puVar6 = puVar6 + 8;
          puVar3 = puVar3 + 8;
        } while (uVar4 != 0);
        if (uVar1 == uVar10) goto LAB_1027ad3fc;
      }
    }
    lVar9 = uVar7 - uVar5;
    puVar3 = local_40 + uVar5;
    puVar6 = (undefined8 *)(uVar5 * 8 + lVar11 + 0x10);
    do {
      *puVar3 = *puVar6;
      lVar9 = lVar9 + -1;
      puVar3 = puVar3 + 1;
      puVar6 = puVar6 + 1;
    } while (lVar9 != 0);
  }
LAB_1027ad3fc:
  param_1[2] = *local_40;
  param_1[3] = local_40[1];
  param_1[4] = local_40[2];
  param_1[5] = local_40[3];
  func_0x000103462eb4();
  uVar12 = *(undefined8 *)(*param_2 + 0x30);
  param_1[7] = *(undefined8 *)(*param_2 + 0x38);
  param_1[6] = uVar12;
  param_1[8] = 0x3ffa876cd8f2a161;
  param_1[0xd] = 0x3ff0000000000000;
  dVar15 = *(double *)(*param_2 + 0x48);
  dVar14 = *(double *)(*param_2 + 0x40);
  param_1[10] = dVar15;
  param_1[9] = dVar14;
  auVar16 = NEON_fmov(0x3ff0000000000000,8);
  param_1[0xc] = auVar16._8_8_ / dVar15;
  param_1[0xb] = auVar16._0_8_ / dVar14;
  return param_1;
}

