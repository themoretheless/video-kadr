
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

undefined8 entry(long param_1,double *param_2,double *param_3)

{
  ulong uVar1;
  ulong uVar2;
  long lVar3;
  long lVar4;
  undefined8 *puVar5;
  ulong uVar6;
  double *pdVar7;
  double dVar8;
  double dVar9;
  double dVar10;
  double dVar11;
  double dVar12;
  double dVar13;
  double dVar14;
  double dVar15;
  double dVar16;
  double dVar17;
  double dVar18;
  double dVar19;
  double dVar20;
  double dVar21;
  double dVar22;
  double dVar23;
  double dVar24;
  double dVar25;
  double dVar26;
  
  uVar2 = *(ulong *)(param_1 + 0x28);
  if (*(char *)(param_1 + 0xf4) == '\0') {
    if (uVar2 == 0) {
      lVar4 = *(long *)(param_1 + 8);
      dVar8 = *param_2;
      *(double *)(param_1 + 0x80) = param_2[1];
      *(double *)(param_1 + 0x78) = dVar8;
      *(double *)(param_1 + 0x88) = param_2[2];
      *(undefined8 *)(param_1 + 0x98) = 0;
      *(undefined8 *)(param_1 + 0xa0) = 0;
      *(undefined8 *)(param_1 + 0x90) = 0;
      dVar8 = *param_3;
      *(double *)(param_1 + 0xb0) = param_3[1];
      *(double *)(param_1 + 0xa8) = dVar8;
      *(double *)(param_1 + 0xb8) = param_3[2];
      *(undefined8 *)(param_1 + 200) = 0;
      *(undefined8 *)(param_1 + 0xd0) = 0;
      *(undefined8 *)(param_1 + 0xc0) = 0;
      uVar2 = 0;
      if (*(long *)(param_1 + 0x10) != lVar4) {
        uVar2 = (*(long *)(param_1 + 0x10) - lVar4 >> 3) * 0xaa - 1;
      }
      uVar6 = *(ulong *)(param_1 + 0x20);
      if (uVar2 == uVar6) {
        func_0x0001027c8960(param_1);
        lVar4 = *(long *)(param_1 + 8);
        uVar6 = *(long *)(param_1 + 0x28) + *(long *)(param_1 + 0x20);
      }
      pdVar7 = (double *)(*(long *)(lVar4 + (uVar6 / 0xaa) * 8) + (uVar6 % 0xaa) * 0x18);
      dVar9 = param_2[1];
      dVar8 = *param_2;
      pdVar7[2] = param_2[2];
      pdVar7[1] = dVar9;
      *pdVar7 = dVar8;
      lVar3 = *(long *)(param_1 + 0x38);
      lVar4 = *(long *)(param_1 + 0x40) - lVar3;
      *(long *)(param_1 + 0x28) = *(long *)(param_1 + 0x28) + 1;
      uVar2 = 0;
      if (lVar4 != 0) {
        uVar2 = (lVar4 >> 3) * 0xaa - 1;
      }
      uVar6 = *(long *)(param_1 + 0x58) + *(long *)(param_1 + 0x50);
      if (uVar2 == uVar6) {
        func_0x0001027c8960(param_1 + 0x30);
        lVar3 = *(long *)(param_1 + 0x38);
        uVar6 = *(long *)(param_1 + 0x58) + *(long *)(param_1 + 0x50);
      }
      pdVar7 = (double *)(*(long *)(lVar3 + (uVar6 / 0xaa) * 8) + (uVar6 % 0xaa) * 0x18);
      dVar9 = param_3[1];
      dVar8 = *param_3;
      pdVar7[2] = param_3[2];
      pdVar7[1] = dVar9;
      *pdVar7 = dVar8;
      *(long *)(param_1 + 0x58) = *(long *)(param_1 + 0x58) + 1;
      return 0xffffffff;
    }
    dVar8 = (double)uVar2;
    dVar9 = (double)(uVar2 + 1);
    dVar15 = (*param_2 + *(double *)(param_1 + 0x78) * dVar8) / dVar9;
    dVar14 = (param_2[1] + *(double *)(param_1 + 0x80) * dVar8) / dVar9;
    dVar20 = (param_2[2] + *(double *)(param_1 + 0x88) * dVar8) / dVar9;
    dVar11 = dVar15 - *(double *)(param_1 + 0x78);
    dVar13 = dVar14 - *(double *)(param_1 + 0x80);
    dVar16 = dVar20 - *(double *)(param_1 + 0x88);
    dVar10 = dVar15 - *param_2;
    dVar12 = dVar14 - param_2[1];
    dVar21 = dVar20 - param_2[2];
    *(double *)(param_1 + 0x80) = dVar14;
    *(double *)(param_1 + 0x78) = dVar15;
    *(double *)(param_1 + 0x88) = dVar20;
    *(double *)(param_1 + 0x98) =
         (dVar12 * dVar12 + (*(double *)(param_1 + 0x98) + dVar13 * dVar13) * dVar8) / dVar9;
    *(double *)(param_1 + 0x90) =
         (dVar10 * dVar10 + (*(double *)(param_1 + 0x90) + dVar11 * dVar11) * dVar8) / dVar9;
    *(double *)(param_1 + 0xa0) =
         (dVar21 * dVar21 + (*(double *)(param_1 + 0xa0) + dVar16 * dVar16) * dVar8) / dVar9;
    dVar15 = (*param_3 + *(double *)(param_1 + 0xa8) * dVar8) / dVar9;
    dVar14 = (param_3[1] + *(double *)(param_1 + 0xb0) * dVar8) / dVar9;
    dVar20 = (param_3[2] + *(double *)(param_1 + 0xb8) * dVar8) / dVar9;
    dVar11 = dVar15 - *(double *)(param_1 + 0xa8);
    dVar13 = dVar14 - *(double *)(param_1 + 0xb0);
    dVar16 = dVar20 - *(double *)(param_1 + 0xb8);
    dVar10 = dVar15 - *param_3;
    dVar12 = dVar14 - param_3[1];
    dVar21 = dVar20 - param_3[2];
    *(double *)(param_1 + 0xb0) = dVar14;
    *(double *)(param_1 + 0xa8) = dVar15;
    *(double *)(param_1 + 0xb8) = dVar20;
    *(double *)(param_1 + 200) =
         (dVar12 * dVar12 + (*(double *)(param_1 + 200) + dVar13 * dVar13) * dVar8) / dVar9;
    *(double *)(param_1 + 0xc0) =
         (dVar10 * dVar10 + (*(double *)(param_1 + 0xc0) + dVar11 * dVar11) * dVar8) / dVar9;
    lVar4 = *(long *)(param_1 + 8);
    *(double *)(param_1 + 0xd0) =
         (dVar21 * dVar21 + (*(double *)(param_1 + 0xd0) + dVar16 * dVar16) * dVar8) / dVar9;
    uVar6 = 0;
    if (*(long *)(param_1 + 0x10) != lVar4) {
      uVar6 = (*(long *)(param_1 + 0x10) - lVar4 >> 3) * 0xaa - 1;
    }
    uVar2 = *(long *)(param_1 + 0x20) + uVar2;
    if (uVar6 == uVar2) {
      func_0x0001027c8960(param_1);
      lVar4 = *(long *)(param_1 + 8);
      uVar2 = *(long *)(param_1 + 0x28) + *(long *)(param_1 + 0x20);
    }
    pdVar7 = (double *)(*(long *)(lVar4 + (uVar2 / 0xaa) * 8) + (uVar2 % 0xaa) * 0x18);
    dVar9 = param_2[1];
    dVar8 = *param_2;
    pdVar7[2] = param_2[2];
    pdVar7[1] = dVar9;
    *pdVar7 = dVar8;
    lVar3 = *(long *)(param_1 + 0x38);
    lVar4 = *(long *)(param_1 + 0x40) - lVar3;
    *(long *)(param_1 + 0x28) = *(long *)(param_1 + 0x28) + 1;
    uVar2 = 0;
    if (lVar4 != 0) {
      uVar2 = (lVar4 >> 3) * 0xaa - 1;
    }
    uVar6 = *(long *)(param_1 + 0x58) + *(long *)(param_1 + 0x50);
    if (uVar2 == uVar6) {
      func_0x0001027c8960(param_1 + 0x30);
      lVar3 = *(long *)(param_1 + 0x38);
      uVar6 = *(long *)(param_1 + 0x58) + *(long *)(param_1 + 0x50);
    }
    pdVar7 = (double *)(*(long *)(lVar3 + (uVar6 / 0xaa) * 8) + (uVar6 % 0xaa) * 0x18);
    dVar9 = param_3[1];
    dVar8 = *param_3;
    pdVar7[2] = param_3[2];
    pdVar7[1] = dVar9;
    *pdVar7 = dVar8;
    *(long *)(param_1 + 0x58) = *(long *)(param_1 + 0x58) + 1;
    if (*(ulong *)(param_1 + 0x28) < (ulong)(long)*(int *)(param_1 + 0xf0)) {
      return 0xffffffff;
    }
    *(undefined1 *)(param_1 + 0xf4) = 1;
    dVar8 = *(double *)(param_1 + 0xe0);
    if (dVar8 <= ABS(*(double *)(param_1 + 0xc0))) {
      return 0;
    }
  }
  else {
    lVar4 = *(long *)(param_1 + 8);
    uVar6 = *(ulong *)(param_1 + 0x20);
    dVar8 = (double)uVar2;
    dVar10 = param_2[1];
    dVar9 = *param_2;
    dVar12 = param_2[2];
    dVar13 = *(double *)(param_1 + 0x80);
    dVar11 = *(double *)(param_1 + 0x78);
    dVar15 = *(double *)(param_1 + 0x88);
    uVar1 = 0;
    if (*(long *)(param_1 + 0x10) != lVar4) {
      uVar1 = (*(long *)(param_1 + 0x10) - lVar4 >> 3) * 0xaa - 1;
    }
    dVar18 = *(double *)(param_1 + 0xb0);
    dVar17 = *(double *)(param_1 + 0xa8);
    dVar19 = *(double *)(param_1 + 0xb8);
    pdVar7 = (double *)(*(long *)(lVar4 + (uVar6 / 0xaa) * 8) + (uVar6 % 0xaa) * 0x18);
    dVar16 = pdVar7[1];
    dVar14 = *pdVar7;
    dVar20 = (dVar9 - dVar14) / dVar8;
    dVar21 = (dVar10 - dVar16) / dVar8;
    dVar22 = pdVar7[2];
    dVar23 = (dVar12 - dVar22) / dVar8;
    dVar24 = dVar20 + dVar11;
    dVar25 = dVar21 + dVar13;
    dVar26 = dVar15 + dVar23;
    *(double *)(param_1 + 0x80) = dVar25;
    *(double *)(param_1 + 0x78) = dVar24;
    *(double *)(param_1 + 0x88) = dVar26;
    *(double *)(param_1 + 0x98) =
         *(double *)(param_1 + 0x98) + dVar21 * (((dVar10 + dVar16) - dVar25) - dVar13);
    *(double *)(param_1 + 0x90) =
         *(double *)(param_1 + 0x90) + dVar20 * (((dVar9 + dVar14) - dVar24) - dVar11);
    *(double *)(param_1 + 0xa0) =
         *(double *)(param_1 + 0xa0) + dVar23 * (((dVar12 + dVar22) - dVar26) - dVar15);
    pdVar7 = (double *)
             (*(long *)(*(long *)(param_1 + 0x38) + (*(ulong *)(param_1 + 0x50) / 0xaa) * 8) +
             (*(ulong *)(param_1 + 0x50) % 0xaa) * 0x18);
    dVar10 = param_3[1];
    dVar9 = *param_3;
    dVar15 = pdVar7[1];
    dVar13 = *pdVar7;
    dVar12 = (dVar9 - dVar13) / dVar8;
    dVar11 = (dVar10 - dVar15) / dVar8;
    dVar14 = param_3[2];
    dVar16 = pdVar7[2];
    dVar8 = (dVar14 - dVar16) / dVar8;
    dVar20 = dVar12 + dVar17;
    dVar21 = dVar11 + dVar18;
    dVar22 = dVar19 + dVar8;
    *(double *)(param_1 + 0xb0) = dVar21;
    *(double *)(param_1 + 0xa8) = dVar20;
    *(double *)(param_1 + 0xb8) = dVar22;
    *(double *)(param_1 + 200) =
         *(double *)(param_1 + 200) + dVar11 * (((dVar10 + dVar15) - dVar21) - dVar18);
    *(double *)(param_1 + 0xc0) =
         *(double *)(param_1 + 0xc0) + dVar12 * (((dVar9 + dVar13) - dVar20) - dVar17);
    *(double *)(param_1 + 0xd0) =
         *(double *)(param_1 + 0xd0) + dVar8 * (((dVar14 + dVar16) - dVar22) - dVar19);
    uVar6 = uVar6 + uVar2;
    if (uVar1 == uVar6) {
      func_0x0001027c8960(param_1);
      lVar4 = *(long *)(param_1 + 8);
      uVar6 = *(long *)(param_1 + 0x28) + *(long *)(param_1 + 0x20);
    }
    pdVar7 = (double *)(*(long *)(lVar4 + (uVar6 / 0xaa) * 8) + (uVar6 % 0xaa) * 0x18);
    dVar9 = param_2[1];
    dVar8 = *param_2;
    pdVar7[2] = param_2[2];
    pdVar7[1] = dVar9;
    *pdVar7 = dVar8;
    lVar3 = *(long *)(param_1 + 0x38);
    lVar4 = *(long *)(param_1 + 0x40) - lVar3;
    *(long *)(param_1 + 0x28) = *(long *)(param_1 + 0x28) + 1;
    uVar2 = 0;
    if (lVar4 != 0) {
      uVar2 = (lVar4 >> 3) * 0xaa - 1;
    }
    uVar6 = *(long *)(param_1 + 0x58) + *(long *)(param_1 + 0x50);
    if (uVar2 == uVar6) {
      func_0x0001027c8960(param_1 + 0x30);
      lVar3 = *(long *)(param_1 + 0x38);
      uVar6 = *(long *)(param_1 + 0x58) + *(long *)(param_1 + 0x50);
    }
    pdVar7 = (double *)(*(long *)(lVar3 + (uVar6 / 0xaa) * 8) + (uVar6 % 0xaa) * 0x18);
    dVar9 = param_3[1];
    dVar8 = *param_3;
    pdVar7[2] = param_3[2];
    pdVar7[1] = dVar9;
    *pdVar7 = dVar8;
    lVar4 = *(long *)(param_1 + 0x58);
    *(long *)(param_1 + 0x58) = lVar4 + 1;
    uVar2 = *(long *)(param_1 + 0x20) + _DAT_1037ca6b0;
    *(long *)(param_1 + 0x28) = *(long *)(param_1 + 0x28) + lRam00000001037ca6b8;
    *(ulong *)(param_1 + 0x20) = uVar2;
    if (uVar2 < 0x154) {
      puVar5 = *(undefined8 **)(param_1 + 0x38);
      uVar2 = *(long *)(param_1 + 0x50) + 1;
      *(ulong *)(param_1 + 0x50) = uVar2;
      *(long *)(param_1 + 0x58) = lVar4;
    }
    else {
      func_0x000103462518(**(undefined8 **)(param_1 + 8));
      *(long *)(param_1 + 8) = *(long *)(param_1 + 8) + 8;
      *(long *)(param_1 + 0x20) = *(long *)(param_1 + 0x20) + -0xaa;
      puVar5 = *(undefined8 **)(param_1 + 0x38);
      uVar2 = *(long *)(param_1 + 0x50) + 1;
      *(ulong *)(param_1 + 0x50) = uVar2;
      *(long *)(param_1 + 0x58) = *(long *)(param_1 + 0x58) + -1;
    }
    if (uVar2 < 0x154) {
      dVar8 = *(double *)(param_1 + 0xe0);
      if (dVar8 <= ABS(*(double *)(param_1 + 0xc0))) {
        return 0;
      }
    }
    else {
      func_0x000103462518(*puVar5);
      *(long *)(param_1 + 0x38) = *(long *)(param_1 + 0x38) + 8;
      *(long *)(param_1 + 0x50) = *(long *)(param_1 + 0x50) + -0xaa;
      dVar8 = *(double *)(param_1 + 0xe0);
      if (dVar8 <= ABS(*(double *)(param_1 + 0xc0))) {
        return 0;
      }
    }
  }
  if ((((ABS(*(double *)(param_1 + 200)) < dVar8) && (ABS(*(double *)(param_1 + 0xd0)) < dVar8)) &&
      (dVar8 = *(double *)(param_1 + 0xd8), ABS(*(double *)(param_1 + 0x90)) < dVar8)) &&
     ((ABS(*(double *)(param_1 + 0x98)) < dVar8 && (ABS(*(double *)(param_1 + 0xa0)) < dVar8)))) {
    *(undefined8 *)(param_1 + 0x68) = *(undefined8 *)(param_1 + 0x80);
    *(undefined8 *)(param_1 + 0x60) = *(undefined8 *)(param_1 + 0x78);
    *(undefined8 *)(param_1 + 0x70) = *(undefined8 *)(param_1 + 0x88);
    return 1;
  }
  return 0;
}

