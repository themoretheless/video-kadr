/* REA/Ghidra pseudocode observation; not original source or product implementation. */

void __ZN3ins19GeometryWrapperBase16fromCameraParamsERKN12Insta360Lens12CameraParamsE
               (long *param_1,long *param_2)

{
  long *plVar1;
  ulong uVar2;
  undefined8 uVar3;
  long lVar4;
  long *plVar5;
  undefined8 *puVar6;
  ulong uVar7;
  long lVar8;
  ulong uVar9;
  long lVar10;
  long lVar11;
  undefined8 *puVar12;
  ulong uVar13;
  ulong uVar14;
  ulong uVar15;
  long lVar16;
  undefined1 auVar17 [16];
  undefined8 uVar18;
  undefined8 uVar19;
  undefined8 uVar20;
  undefined8 uVar21;
  undefined8 uVar22;
  undefined8 uVar23;
  
  lVar4 = param_2[1];
  *param_1 = 0;
  param_1[1] = 0;
  lVar4 = lVar4 << 0x20;
  FUN_1027b0434(param_1,lVar4 + 0x700000000 >> 0x20);
  plVar5 = (long *)*param_1;
  plVar1 = param_2 + 6;
  uVar9 = (ulong)plVar5 >> 3 & 1;
  if (((ulong)plVar5 & 7) != 0) {
    uVar9 = 2;
  }
  uVar13 = uVar9 + (ulong)(uVar9 == 0) * 2;
  if (uVar9 == 0) {
    lVar10 = *plVar1;
    plVar5[1] = param_2[7];
    *plVar5 = lVar10;
    if (2 < uVar13) {
      lVar10 = param_2[8];
      plVar5[3] = param_2[9];
      plVar5[2] = lVar10;
      if (4 < uVar13) {
        lVar10 = param_2[10];
        plVar5[5] = param_2[0xb];
        plVar5[4] = lVar10;
      }
    }
  }
  else {
    *plVar5 = *plVar1;
    if ((uVar9 != 1) && (plVar5[1] = param_2[7], ((ulong)plVar5 & 7) == 0)) {
      plVar5[2] = param_2[8];
    }
  }
  if (uVar13 < 2) {
    lVar10 = uVar13 * 8;
    do {
      *(undefined8 *)((long)plVar5 + lVar10) = *(undefined8 *)((long)plVar1 + lVar10);
      lVar10 = lVar10 + 8;
    } while (lVar10 != 0x10);
  }
  uVar7 = lVar4 >> 0x20;
  lVar10 = *param_1;
  uVar9 = lVar10 + 0x10;
  lVar11 = *param_2;
  uVar13 = uVar9 >> 3 & 1;
  if ((long)uVar7 <= (long)uVar13) {
    uVar13 = uVar7;
  }
  if ((uVar9 & 7) != 0) {
    uVar13 = uVar7;
  }
  uVar14 = uVar7 - uVar13;
  uVar2 = uVar14;
  if ((long)uVar7 < (long)uVar13) {
    uVar2 = uVar14 + 1;
  }
  if (0 < (long)uVar13) {
    uVar15 = 0;
    if ((7 < uVar13) && (0x3f < uVar9 - lVar11)) {
      uVar15 = uVar13 & 0xfffffffffffffff8;
      puVar6 = (undefined8 *)(lVar10 + 0x20);
      uVar9 = uVar15;
      puVar12 = (undefined8 *)(lVar11 + 0x20);
      do {
        uVar3 = puVar12[-4];
        uVar19 = puVar12[-1];
        uVar18 = puVar12[-2];
        uVar21 = puVar12[1];
        uVar20 = *puVar12;
        uVar23 = puVar12[3];
        uVar22 = puVar12[2];
        puVar6[-1] = puVar12[-3];
        puVar6[-2] = uVar3;
        puVar6[1] = uVar19;
        *puVar6 = uVar18;
        puVar6[3] = uVar21;
        puVar6[2] = uVar20;
        puVar6[5] = uVar23;
        puVar6[4] = uVar22;
        puVar6 = puVar6 + 8;
        uVar9 = uVar9 - 8;
        puVar12 = puVar12 + 8;
      } while (uVar9 != 0);
      if (uVar13 == uVar15) goto LAB_1027ab6e0;
    }
    lVar16 = uVar13 - uVar15;
    puVar6 = (undefined8 *)(lVar11 + uVar15 * 8);
    puVar12 = (undefined8 *)(uVar15 * 8 + lVar10 + 0x10);
    do {
      *puVar12 = *puVar6;
      lVar16 = lVar16 + -1;
      puVar6 = puVar6 + 1;
      puVar12 = puVar12 + 1;
    } while (lVar16 != 0);
  }
LAB_1027ab6e0:
  lVar16 = (uVar2 & 0xfffffffffffffffe) + uVar13;
  if (1 < (long)uVar14) {
    puVar12 = (undefined8 *)(lVar11 + uVar13 * 8);
    uVar9 = uVar13;
    puVar6 = (undefined8 *)(uVar13 * 8 + lVar10);
    do {
      uVar3 = *puVar12;
      puVar6[3] = puVar12[1];
      puVar6[2] = uVar3;
      uVar9 = uVar9 + 2;
      puVar12 = puVar12 + 2;
      puVar6 = puVar6 + 2;
    } while ((long)uVar9 < lVar16);
  }
  uVar9 = uVar7 - lVar16;
  if (uVar9 != 0 && lVar16 <= (long)uVar7) {
    if (0xf < uVar9) {
      if (0x3f < (lVar10 - lVar11) + 0x10U) {
        uVar14 = uVar9 & 0xfffffffffffffff8;
        lVar16 = lVar16 + uVar14;
        lVar8 = ((long)uVar2 >> 1) * 0x10 + uVar13 * 8 + 0x20;
        puVar6 = (undefined8 *)(lVar10 + lVar8);
        puVar12 = (undefined8 *)(lVar11 + lVar8);
        uVar13 = uVar14;
        do {
          uVar3 = puVar12[-4];
          uVar19 = puVar12[-1];
          uVar18 = puVar12[-2];
          uVar21 = puVar12[1];
          uVar20 = *puVar12;
          uVar23 = puVar12[3];
          uVar22 = puVar12[2];
          puVar6[-1] = puVar12[-3];
          puVar6[-2] = uVar3;
          puVar6[1] = uVar19;
          *puVar6 = uVar18;
          puVar6[3] = uVar21;
          puVar6[2] = uVar20;
          puVar6[5] = uVar23;
          puVar6[4] = uVar22;
          puVar6 = puVar6 + 8;
          uVar13 = uVar13 - 8;
          puVar12 = puVar12 + 8;
        } while (uVar13 != 0);
        if (uVar9 == uVar14) goto LAB_1027ab7b4;
      }
    }
    lVar8 = uVar7 - lVar16;
    puVar6 = (undefined8 *)(lVar16 * 8 + lVar10 + 0x10);
    puVar12 = (undefined8 *)(lVar11 + lVar16 * 8);
    do {
      *puVar6 = *puVar12;
      lVar8 = lVar8 + -1;
      puVar6 = puVar6 + 1;
      puVar12 = puVar12 + 1;
    } while (lVar8 != 0);
  }
LAB_1027ab7b4:
  lVar10 = *param_1;
  auVar17._0_8_ = (long)(int)param_2[2];
  auVar17._8_8_ = (long)(int)((ulong)param_2[2] >> 0x20);
  auVar17 = NEON_scvtf(auVar17,8);
  puVar6 = (undefined8 *)(lVar10 + (lVar4 + 0x200000000 >> 0x1d));
  puVar6[1] = auVar17._8_8_;
  *puVar6 = auVar17._0_8_;
  *(long *)(lVar10 + (lVar4 + 0x400000000 >> 0x1d)) = param_2[3];
  *(long *)(lVar10 + (lVar4 + 0x500000000 >> 0x1d)) = param_2[4];
  *(long *)(lVar10 + (lVar4 + 0x600000000 >> 0x1d)) = param_2[8];
  return;
}

