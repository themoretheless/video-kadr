/* REA/Ghidra function-only observation; not original source or product code. */

/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

undefined8 * entry(undefined8 *param_1,long *param_2,ulong param_3)

{
  ulong uVar1;
  undefined8 uVar2;
  undefined8 *puVar3;
  ulong uVar4;
  long lVar5;
  ulong uVar6;
  undefined8 *puVar7;
  ulong uVar8;
  ulong uVar9;
  undefined8 *puVar10;
  undefined8 uVar11;
  undefined8 uVar12;
  double dVar13;
  double dVar14;
  undefined1 auVar15 [16];
  undefined8 uVar16;
  undefined8 uVar17;
  undefined8 uVar18;
  undefined8 uVar19;
  undefined8 *local_50;
  ulong local_48;
  
  uVar11 = *(undefined8 *)*param_2;
  param_1[1] = ((undefined8 *)*param_2)[1];
  *param_1 = uVar11;
  uVar9 = (ulong)(int)param_3;
  puVar10 = (undefined8 *)*param_2;
  local_50 = (undefined8 *)0x0;
  local_48 = 0;
  func_0x0001013bb6c0(&local_50,uVar9,1);
  if (local_48 != uVar9) {
    func_0x0001013bb6c0(&local_50,uVar9,1);
    uVar9 = local_48;
  }
  uVar8 = uVar9;
  if ((long)uVar9 < 0) {
    uVar8 = uVar9 + 1;
  }
  uVar4 = uVar8 & 0xfffffffffffffffe;
  if (1 < (long)uVar9) {
    lVar5 = 0;
    puVar7 = local_50;
    puVar3 = puVar10;
    do {
      uVar11 = puVar3[2];
      puVar7[1] = puVar3[3];
      *puVar7 = uVar11;
      lVar5 = lVar5 + 2;
      puVar7 = puVar7 + 2;
      puVar3 = puVar3 + 2;
    } while (lVar5 < (long)uVar4);
  }
  uVar1 = (long)uVar9 % 2;
  if (uVar1 != 0 && (long)uVar4 <= (long)uVar9) {
    if (9 < uVar1) {
      lVar5 = (long)uVar8 >> 1;
      if (0x3f < (ulong)((long)local_50 + (lVar5 * 0x10 - (long)(puVar10 + lVar5 * 2)) + -0x10)) {
        uVar6 = uVar1 & 0xfffffffffffffff8;
        uVar4 = uVar4 + uVar6;
        puVar7 = local_50 + lVar5 * 2 + 4;
        uVar8 = uVar6;
        puVar3 = puVar10 + lVar5 * 2;
        do {
          uVar12 = puVar3[2];
          uVar11 = puVar3[4];
          uVar2 = puVar3[5];
          uVar17 = puVar3[7];
          uVar16 = puVar3[6];
          uVar19 = puVar3[9];
          uVar18 = puVar3[8];
          puVar7[-3] = puVar3[3];
          puVar7[-4] = uVar12;
          puVar7[-1] = uVar2;
          puVar7[-2] = uVar11;
          puVar7[1] = uVar17;
          *puVar7 = uVar16;
          puVar7[3] = uVar19;
          puVar7[2] = uVar18;
          uVar8 = uVar8 - 8;
          puVar7 = puVar7 + 8;
          puVar3 = puVar3 + 8;
        } while (uVar8 != 0);
        if (uVar1 == uVar6) goto LAB_1027af79c;
      }
    }
    lVar5 = uVar9 - uVar4;
    puVar3 = local_50 + uVar4;
    puVar10 = puVar10 + uVar4 + 2;
    do {
      *puVar3 = *puVar10;
      lVar5 = lVar5 + -1;
      puVar3 = puVar3 + 1;
      puVar10 = puVar10 + 1;
    } while (lVar5 != 0);
  }
LAB_1027af79c:
  param_1[2] = *local_50;
  param_1[3] = local_50[1];
  param_1[4] = local_50[2];
  param_1[5] = local_50[3];
  param_1[6] = local_50[4];
  param_1[7] = local_50[5];
  param_1[8] = local_50[6];
  param_1[9] = local_50[7];
  param_1[10] = local_50[8];
  param_1[0xb] = local_50[9];
  param_1[0xc] = local_50[10];
  param_1[0xd] = local_50[0xb];
  param_1[0xe] = local_50[0xc];
  func_0x000103462eb4();
  uVar9 = -(param_3 >> 0x1f & 1) & 0xfffffff800000000 | (param_3 & 0xffffffff) << 3;
  lVar5 = uVar9 + *param_2;
  uVar11 = *(undefined8 *)(lVar5 + 0x10);
  param_1[0x11] = *(undefined8 *)(lVar5 + 0x18);
  param_1[0x10] = uVar11;
  lVar5 = uVar9 + *param_2;
  param_1[0x16] = *(undefined8 *)(lVar5 + 0x30);
  uVar11 = _DAT_103c120d0;
  param_1[0x18] = uRam0000000103c120d8;
  param_1[0x17] = uVar11;
  dVar13 = *(double *)(lVar5 + 0x20);
  dVar14 = *(double *)(lVar5 + 0x28);
  param_1[0x12] = dVar13;
  param_1[0x13] = dVar14;
  auVar15 = NEON_fmov(0x3ff0000000000000,8);
  param_1[0x15] = auVar15._8_8_ / dVar14;
  param_1[0x14] = auVar15._0_8_ / dVar13;
  return param_1;
}

