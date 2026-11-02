
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

undefined8 entry(long *param_1,long *param_2,long *param_3,long *param_4,double *param_5)

{
  undefined1 auVar1 [16];
  code *pcVar2;
  undefined8 uVar3;
  double *pdVar4;
  long lVar5;
  long lVar6;
  long lVar7;
  ulong uVar8;
  double dVar9;
  double dVar10;
  double dVar11;
  double dVar12;
  double dVar13;
  double dVar14;
  double dVar15;
  undefined1 auVar16 [16];
  double local_f0;
  double dStack_e8;
  undefined8 local_e0;
  undefined8 uStack_d8;
  double local_d0;
  double dStack_c8;
  double local_c0;
  double dStack_b8;
  double local_a0;
  double dStack_98;
  double local_90;
  double local_88;
  long local_78;
  long local_70;
  long lStack_68;
  
  local_70 = 0;
  lStack_68 = 0;
  local_78 = 0;
  uVar8 = param_2[1] - *param_2;
  if (uVar8 != 0) {
    if (0x3ffffffffffffff8 < uVar8) {
      func_0x000100023e9c(&local_78);
                    /* WARNING: Does not return */
      pcVar2 = (code *)SoftwareBreakpoint(1,0x101b55eb0);
      (*pcVar2)();
    }
    local_78 = func_0x000103462620(uVar8 * 4);
    lStack_68 = local_78 + ((long)uVar8 >> 3) * 0x20;
    local_70 = local_78 + uVar8 * 4;
  }
  func_0x0001027b71c8(*(undefined8 *)param_1[0xe],param_2,&local_78);
  if ((char)param_1[0x1a] == '\0') {
    dStack_e8 = (double)param_1[0xb];
    local_f0 = (double)param_1[10];
    dVar11 = (double)param_1[0xd];
    dVar9 = (double)param_1[0xc];
  }
  else {
    pdVar4 = (double *)param_1[6];
    dVar9 = *pdVar4;
    dVar10 = pdVar4[3];
    dVar13 = pdVar4[2];
    dVar12 = pdVar4[1];
    dVar11 = dVar9 * 0.0 + dVar10 * -0.7071067811865476 + dVar12 * 0.7071067811865476 + dVar13 * 0.0
    ;
    dVar14 = dVar12 * 0.7071067811865476 + _DAT_103b46c50 * dVar10;
    dVar15 = dVar9 * 0.7071067811865476 + dRam0000000103b46c58 * dVar10;
    auVar16._8_8_ = dVar15;
    auVar16._0_8_ = dVar14;
    auVar1._8_8_ = dVar15;
    auVar1._0_8_ = dVar14;
    auVar16 = NEON_ext(auVar16,auVar1,8,1);
    local_f0 = auVar16._0_8_ + dVar12 * -0.0 + _DAT_103b46c50 * dVar13;
    dStack_e8 = auVar16._8_8_ + dVar13 * -0.0 + dRam0000000103b46c58 * dVar9;
    dVar9 = dVar13 * 0.7071067811865476 + dVar10 * 0.0 + dVar9 * -0.7071067811865476 + dVar12 * 0.0;
  }
  func_0x0001027b04b0(&local_d0,&local_78);
  local_88 = local_d0 * local_f0 + dStack_b8 * dVar11 + dStack_c8 * dStack_e8 + local_c0 * dVar9;
  local_a0 = ((-local_d0 * dVar11 + dStack_b8 * local_f0) - dStack_c8 * dVar9) +
             local_c0 * dStack_e8;
  dStack_98 = ((-dStack_c8 * dVar11 + dStack_b8 * dStack_e8) - local_c0 * local_f0) +
              local_d0 * dVar9;
  local_90 = ((-local_c0 * dVar11 + dStack_b8 * dVar9) - local_d0 * dStack_e8) +
             dStack_c8 * local_f0;
  if ((param_4[1] == (param_1[1] - *param_1 >> 4) * (long)*(int *)((long)param_1 + 0xa4)) &&
     (param_4[2] == (long)(int)param_1[0x14])) {
    lVar5 = *param_2;
    if (param_2[1] == lVar5) {
      uVar3 = 0;
    }
    else {
      lVar6 = 0;
      lVar7 = 0;
      uVar8 = 0;
      do {
        local_c0 = (double)(long)(int)param_1[0x14];
        dStack_c8 = (double)(long)*(int *)((long)param_1 + 0xa4);
        dStack_b8 = (double)param_4[3];
        local_d0 = (double)(*param_4 + uVar8 * (long)dStack_c8 * (long)dStack_b8 * 8);
        *(int *)(param_1 + 0x13) = (int)param_1[0x14];
        *(int *)((long)param_1 + 0x9c) = *(int *)((long)param_1 + 0xa4);
        local_e0 = 0;
        uStack_d8 = 0;
        uVar3 = func_0x000101b4fcb4(*(undefined8 *)(lVar5 + uVar8 * 8),param_1[0x15],
                                    *(undefined8 *)(*param_1 + lVar7),&local_d0,&local_a0,
                                    param_1[6] + lVar6,*(undefined1 *)(*param_3 + uVar8),&local_e0);
        lVar5 = *param_2;
        if ((int)uVar3 == -1) break;
        uVar8 = uVar8 + 1;
        lVar7 = lVar7 + 0x10;
        lVar6 = lVar6 + 0x20;
      } while (uVar8 < (ulong)(param_2[1] - lVar5 >> 3));
      if (param_2[1] != lVar5) {
        lVar5 = 0;
        uVar8 = 0;
        do {
          func_0x000101b510e4(*(undefined8 *)(*param_1 + lVar5));
          uVar8 = uVar8 + 1;
          lVar5 = lVar5 + 0x10;
        } while (uVar8 < (ulong)(param_2[1] - *param_2 >> 3));
      }
    }
    param_5[1] = -dStack_98;
    *param_5 = -local_a0;
    param_5[3] = local_88;
    param_5[2] = -local_90;
  }
  else {
    uVar3 = 0xfffffffe;
  }
  if (local_78 != 0) {
    local_70 = local_78;
    func_0x000103462518();
  }
  return uVar3;
}

