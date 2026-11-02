
undefined8
entry(long param_1,double param_2,double param_3,long param_4,undefined8 param_5,double *param_6,
     double *param_7,undefined8 param_8,undefined8 param_9)

{
  long *plVar1;
  int iVar2;
  long lVar3;
  double *pdVar4;
  ulong uVar5;
  long *plVar6;
  undefined1 auVar7 [16];
  undefined1 auVar8 [16];
  double dVar9;
  double dVar10;
  double dVar11;
  double dVar12;
  double dVar13;
  double dVar14;
  double dVar15;
  double dVar16;
  undefined8 uVar17;
  long *plVar18;
  double local_a0;
  double dStack_98;
  double local_90;
  double dStack_88;
  double local_80;
  double dStack_78;
  double dStack_70;
  double dStack_68;
  
  lVar3 = *(long *)(param_4 + 0xc0);
  if (lVar3 != 0) {
    plVar6 = *(long **)(param_4 + 200);
    if (plVar6 != (long *)0x0) {
      param_1 = plVar6[1];
      plVar6[1] = param_1 + 1;
    }
    auVar7._8_8_ = 0;
    auVar7._0_8_ = param_2;
    func_0x0001027b71c0(param_1,auVar7,lVar3,&local_80);
    dVar9 = *param_7;
    dVar10 = param_7[1];
    dVar13 = param_7[2];
    dVar12 = param_7[3];
    dVar11 = ((-dVar9 * local_80 + dVar12 * dStack_68) - dVar10 * dStack_78) - dVar13 * dStack_70;
    dVar15 = (dVar9 * dStack_68 + dVar12 * local_80 + dVar10 * dStack_70) - dVar13 * dStack_78;
    dVar16 = (dVar10 * dStack_68 + dVar12 * dStack_78 + dVar13 * local_80) - dVar9 * dStack_70;
    dVar14 = (dVar13 * dStack_68 + dVar12 * dStack_70 + dVar9 * dStack_78) - dVar10 * local_80;
    dVar9 = *param_6;
    dVar10 = param_6[1];
    dVar13 = param_6[2];
    dVar12 = param_6[3];
    dStack_88 = ((-(dVar15 * dVar9) + dVar12 * dVar11) - dVar10 * dVar16) - dVar13 * dVar14;
    local_a0 = (dVar15 * dVar12 + dVar9 * dVar11 + dVar13 * dVar16) - dVar10 * dVar14;
    dStack_98 = (dVar12 * dVar16 + dVar10 * dVar11 + dVar9 * dVar14) - dVar13 * dVar15;
    local_90 = (dVar12 * dVar14 + dVar13 * dVar11 + dVar10 * dVar15) - dVar9 * dVar16;
    if (plVar6 != (long *)0x0) {
      LOAcquire();
      lVar3 = plVar6[1];
      plVar6[1] = lVar3 + -1;
      LORelease();
      if (lVar3 == 0) {
        (**(code **)(*plVar6 + 0x10))(plVar6);
        func_0x000103462194(plVar6);
      }
    }
    param_2 = param_2 + param_3 * -0.5;
    plVar6 = (long *)(param_4 + 0x90);
    pdVar4 = (double *)*plVar6;
    uVar5 = *(long *)(param_4 + 0x98) - (long)pdVar4;
    if (uVar5 < 0x198) {
      func_0x00010031d5b0(plVar6,0x33 - (uVar5 >> 3));
      pdVar4 = (double *)*plVar6;
    }
    else if (uVar5 != 0x198) {
      *(double **)(param_4 + 0x98) = pdVar4 + 0x33;
    }
    *pdVar4 = param_2 + param_3 * 0.0;
    pdVar4[1] = param_2 + param_3 * 0.02;
    pdVar4[2] = param_2 + param_3 * 0.04;
    pdVar4[3] = param_2 + param_3 * 0.06;
    pdVar4[4] = param_2 + param_3 * 0.08;
    pdVar4[5] = param_2 + param_3 * 0.1;
    pdVar4[6] = param_2 + param_3 * 0.12;
    pdVar4[7] = param_2 + param_3 * 0.14;
    pdVar4[8] = param_2 + param_3 * 0.16;
    pdVar4[9] = param_2 + param_3 * 0.18;
    pdVar4[10] = param_2 + param_3 * 0.2;
    pdVar4[0xb] = param_2 + param_3 * 0.22;
    pdVar4[0xc] = param_2 + param_3 * 0.24;
    pdVar4[0xd] = param_2 + param_3 * 0.26;
    pdVar4[0xe] = param_2 + param_3 * 0.28;
    pdVar4[0xf] = param_2 + param_3 * 0.3;
    pdVar4[0x10] = param_2 + param_3 * 0.32;
    pdVar4[0x11] = param_2 + param_3 * 0.34;
    pdVar4[0x12] = param_2 + param_3 * 0.36;
    pdVar4[0x13] = param_2 + param_3 * 0.38;
    pdVar4[0x14] = param_2 + param_3 * 0.4;
    pdVar4[0x15] = param_2 + param_3 * 0.42;
    pdVar4[0x16] = param_2 + param_3 * 0.44;
    pdVar4[0x17] = param_2 + param_3 * 0.46;
    pdVar4[0x18] = param_2 + param_3 * 0.48;
    pdVar4[0x19] = param_2 + param_3 * 0.5;
    pdVar4[0x1a] = param_2 + param_3 * 0.52;
    pdVar4[0x1b] = param_2 + param_3 * 0.54;
    pdVar4[0x1c] = param_2 + param_3 * 0.56;
    pdVar4[0x1d] = param_2 + param_3 * 0.58;
    pdVar4[0x1e] = param_2 + param_3 * 0.6;
    pdVar4[0x1f] = param_2 + param_3 * 0.62;
    pdVar4[0x20] = param_2 + param_3 * 0.64;
    pdVar4[0x21] = param_2 + param_3 * 0.66;
    pdVar4[0x22] = param_2 + param_3 * 0.68;
    pdVar4[0x23] = param_2 + param_3 * 0.7000000000000001;
    pdVar4[0x24] = param_2 + param_3 * 0.72;
    pdVar4[0x25] = param_2 + param_3 * 0.74;
    pdVar4[0x26] = param_2 + param_3 * 0.76;
    pdVar4[0x27] = param_2 + param_3 * 0.78;
    pdVar4[0x28] = param_2 + param_3 * 0.8;
    pdVar4[0x29] = param_2 + param_3 * 0.8200000000000001;
    pdVar4[0x2a] = param_2 + param_3 * 0.84;
    pdVar4[0x2b] = param_2 + param_3 * 0.86;
    pdVar4[0x2c] = param_2 + param_3 * 0.88;
    pdVar4[0x2d] = param_2 + param_3 * 0.9;
    pdVar4[0x2e] = param_2 + param_3 * 0.92;
    pdVar4[0x2f] = param_2 + param_3 * 0.9400000000000001;
    pdVar4[0x30] = param_2 + param_3 * 0.96;
    pdVar4[0x31] = param_2 + param_3 * 0.98;
    pdVar4[0x32] = param_2 + param_3;
    uVar17 = *(undefined8 *)(param_4 + 0xc0);
    plVar1 = *(long **)(param_4 + 200);
    if (plVar1 != (long *)0x0) {
      pdVar4 = (double *)plVar1[1];
      plVar1[1] = (long)pdVar4 + 1;
    }
    plVar18 = plVar1;
    iVar2 = func_0x0001027b71c8(pdVar4,uVar17,plVar6,param_4 + 0xa8);
    if (plVar1 != (long *)0x0) {
      LOAcquire();
      lVar3 = plVar1[1];
      plVar1[1] = lVar3 + -1;
      LORelease();
      if (lVar3 == 0) {
        (**(code **)(*plVar1 + 0x10))(plVar1);
        func_0x000103462194(plVar1);
      }
    }
    if (iVar2 != -1) {
      if (*(double *)(param_4 + 0x88) == 0.0) {
        auVar8._0_8_ = (long)(int)*(undefined8 *)(param_4 + 0x60);
        auVar8._8_8_ = (long)(int)((ulong)*(undefined8 *)(param_4 + 0x60) >> 0x20);
        auVar7 = NEON_scvtf(auVar8,8);
        auVar8 = NEON_fmov(0x3ff0000000000000,8);
        local_80 = auVar8._0_8_ / auVar7._0_8_;
        dStack_78 = auVar8._8_8_ / auVar7._8_8_;
        iVar2 = *(int *)(param_4 + 0x178);
      }
      else {
        auVar7 = NEON_fmov(0x3ff0000000000000,8);
        dStack_78 = auVar7._8_8_;
        local_80 = auVar7._0_8_;
        iVar2 = *(int *)(param_4 + 0x178);
      }
      if ((iVar2 == 0) || (iVar2 != 1)) {
        func_0x000101b50538(param_4,param_5,param_6,param_7,&local_a0,param_8,&local_80,param_9);
      }
      else {
        func_0x000101b50450(param_4,param_5,param_6,param_7,&local_a0,param_8,&local_80,param_9,
                            uVar17,plVar18);
      }
      return 0;
    }
  }
  return 0xffffffff;
}

