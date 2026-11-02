/* REA/Ghidra function-only observation; not original source or product code. */

/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(long *param_1,undefined8 param_2,int *param_3,double *param_4,long *param_5,
          undefined8 param_6,int param_7)

{
  long lVar1;
  long lVar2;
  long lVar3;
  long *plVar4;
  long lVar5;
  long lVar6;
  long lVar7;
  int iVar8;
  long lVar9;
  long lVar10;
  long lVar11;
  long lVar12;
  double dVar13;
  double dVar14;
  undefined1 auVar15 [16];
  undefined1 auVar16 [16];
  undefined1 auVar17 [16];
  undefined1 auVar18 [16];
  undefined1 auVar19 [16];
  undefined1 auVar20 [16];
  double dVar21;
  double dVar22;
  double dVar23;
  double dVar24;
  double dVar25;
  double dVar26;
  double dVar27;
  double dVar28;
  double dVar29;
  double dVar30;
  double dVar31;
  double dVar32;
  double dVar33;
  double dVar34;
  double local_170;
  double dStack_168;
  long local_158;
  long *local_100;
  long *local_f8;
  double local_f0;
  double dStack_e8;
  double local_e0;
  double local_d8;
  double local_d0;
  double dStack_c8;
  double local_c0;
  double dStack_b8;
  double local_b0;
  
  lVar10 = param_1[1];
  if (lVar10 != 0) {
    lVar6 = param_5[1];
    lVar2 = param_5[2];
    if (param_7 == 0) {
      dStack_168 = 0.0;
      local_170 = 0.0;
      dVar23 = dRam00000001037c4eb8;
      dVar21 = _DAT_1037c4eb0;
      lVar5 = lVar10;
    }
    else {
      func_0x000101032fdc(&local_f0,*param_1 + 0x50);
      dVar24 = (local_f0 * -0.0 + local_d8 * 0.0 + dStack_e8 * -0.0) - local_e0;
      dVar27 = dStack_e8 + local_f0 * 0.0 + local_d8 * 0.0 + local_e0 * -0.0;
      dVar30 = (dStack_e8 * 0.0 + local_d8 * 0.0 + local_e0 * 0.0) - local_f0;
      dVar21 = local_d8 + local_e0 * 0.0 + local_f0 * 0.0 + dStack_e8 * -0.0;
      dVar23 = dVar27 * 0.0 + dVar24 * -0.7071067811865476 + dVar30 * 0.7071067811865476 +
               dVar21 * 0.0;
      local_170 = dVar27 * 0.7071067811865476 + dVar24 * 0.0 + dVar30 * -0.0 +
                  dVar21 * 0.7071067811865476;
      dStack_168 = dVar30 * 0.7071067811865476 + dVar24 * 0.7071067811865476 + dVar21 * -0.0 +
                   dVar27 * 0.0;
      dVar21 = dVar21 * 0.7071067811865476 + dVar24 * 0.0 + dVar27 * -0.7071067811865476 +
               dVar30 * 0.0;
      lVar5 = param_1[1];
    }
    if (0 < lVar5) {
      local_158 = 0;
      lVar5 = 0;
      lVar3 = 0;
      if ((long)(int)lVar10 != 0) {
        lVar3 = lVar6 / (long)(int)lVar10;
      }
      iVar8 = (int)lVar2;
      lVar6 = (long)(int)lVar3;
      lVar10 = (long)iVar8;
      if (lVar10 < 2) {
        lVar10 = 1;
      }
      lVar1 = lVar6;
      if (lVar6 < 2) {
        lVar1 = 1;
      }
      auVar15 = NEON_fmov(0x3ff0000000000000,8);
      do {
        func_0x0001027ab568(&local_f0,*param_1 + lVar5 * 0xc0);
        func_0x0001027aa800(&local_100,&local_f0,*(undefined4 *)(*param_1 + lVar5 * 0xc0 + 0x4c));
        func_0x000103462eb4(local_f0);
        if (*(int *)(*param_1 + lVar5 * 0xc0 + 0xb4) != 0) {
          (**(code **)(*local_100 + 0x40))(local_100,1);
        }
        auVar18._0_8_ = (double)(long)*param_3;
        auVar18._8_8_ = 0;
        (**(code **)(*local_100 + 0x18))(auVar18,(double)(long)param_3[1]);
        dVar24 = (double)func_0x000101b55414(*(undefined4 *)(*param_1 + lVar5 * 0xc0 + 0x48));
        auVar16._0_8_ = ((dVar24 * 3.141592653589793) / 180.0) * 1.05;
        auVar16._8_8_ = 0;
        (**(code **)(*local_100 + 0x48))(auVar16);
        func_0x000101032fdc(&local_f0,*param_1 + lVar5 * 0xc0 + 0x50);
        plVar4 = local_100;
        if ((lVar5 == 0) && (param_1[1] == 2)) {
          dVar27 = (local_f0 * -0.0 + local_d8 * 0.0 + dStack_e8 * -0.0) - local_e0;
          dVar26 = dStack_e8 + local_f0 * 0.0 + local_d8 * 0.0 + local_e0 * -0.0;
          dVar28 = (dStack_e8 * 0.0 + local_d8 * 0.0 + local_e0 * 0.0) - local_f0;
          dVar24 = local_d8 + local_e0 * 0.0 + local_f0 * 0.0 + dStack_e8 * -0.0;
          dVar30 = ((-(dVar26 * local_170) + dVar23 * dVar27) - dStack_168 * dVar28) -
                   dVar21 * dVar24;
          dVar25 = (dVar23 * dVar26 + local_170 * dVar27 + dVar21 * dVar28) - dStack_168 * dVar24;
          dVar31 = (dVar23 * dVar28 + dStack_168 * dVar27 + local_170 * dVar24) - dVar21 * dVar26;
          dVar24 = (dVar23 * dVar24 + dVar21 * dVar27 + dStack_168 * dVar26) - local_170 * dVar28;
          dVar27 = *param_4;
          dVar26 = param_4[1];
          dVar34 = param_4[2];
          dVar28 = param_4[3];
          local_d8 = ((-(dVar25 * dVar27) + dVar28 * dVar30) - dVar26 * dVar31) - dVar34 * dVar24;
          local_f0 = (dVar28 * dVar25 + dVar27 * dVar30 + dVar34 * dVar31) - dVar26 * dVar24;
          dStack_e8 = (dVar28 * dVar31 + dVar26 * dVar30 + dVar27 * dVar24) - dVar34 * dVar25;
          local_e0 = (dVar28 * dVar24 + dVar34 * dVar30 + dVar26 * dVar25) - dVar27 * dVar31;
        }
        else {
          dVar30 = ((-(local_f0 * local_170) + dVar23 * local_d8) - dStack_168 * dStack_e8) -
                   dVar21 * local_e0;
          dVar25 = (dVar23 * local_f0 + local_170 * local_d8 + dVar21 * dStack_e8) -
                   dStack_168 * local_e0;
          dVar26 = (dVar23 * dStack_e8 + dStack_168 * local_d8 + local_170 * local_e0) -
                   dVar21 * local_f0;
          dVar24 = (dVar23 * local_e0 + dVar21 * local_d8 + dStack_168 * local_f0) -
                   local_170 * dStack_e8;
          dVar27 = *param_4;
          dVar28 = param_4[1];
          dVar34 = param_4[2];
          dVar31 = param_4[3];
          local_d8 = ((-(dVar25 * dVar27) + dVar31 * dVar30) - dVar28 * dVar26) - dVar34 * dVar24;
          local_f0 = (dVar31 * dVar25 + dVar27 * dVar30 + dVar34 * dVar26) - dVar28 * dVar24;
          dStack_e8 = (dVar31 * dVar26 + dVar28 * dVar30 + dVar27 * dVar24) - dVar34 * dVar25;
          local_e0 = (dVar31 * dVar24 + dVar34 * dVar30 + dVar28 * dVar25) - dVar27 * dVar26;
        }
        if (0 < lVar3 << 0x20 && 0 < lVar2 << 0x20) {
          lVar11 = 0;
          lVar7 = param_5[3];
          dVar34 = dStack_e8 + dStack_e8;
          dVar28 = local_e0 + local_e0;
          dVar29 = (local_f0 + local_f0) * local_d8;
          dVar32 = dVar34 * local_d8;
          dVar27 = dVar28 * local_d8;
          dVar31 = local_f0 * (local_f0 + local_f0);
          dVar33 = local_f0 * dVar34;
          dVar25 = local_f0 * dVar28;
          dVar34 = dStack_e8 * dVar34;
          dVar26 = dStack_e8 * dVar28;
          dVar28 = local_e0 * dVar28;
          auVar17._0_8_ = (long)(int)*(undefined8 *)param_3 + -1;
          auVar17._8_8_ = (long)(int)((ulong)*(undefined8 *)param_3 >> 0x20) + -1;
          auVar18 = NEON_ucvtf(auVar17,8);
          dVar30 = auVar15._0_8_ / auVar18._0_8_;
          lVar9 = *param_5 + lVar7 * local_158;
          dVar24 = auVar15._0_8_;
          do {
            auVar19._0_8_ =
                 (double)lVar11 * (-3.141592653589793 / (double)((int)lVar3 + -1)) +
                 1.5707963267948966;
            auVar19._8_8_ = 0;
            dVar13 = (double)func_0x0001034627b8(auVar19);
            lVar12 = 0;
            do {
              local_d0 = (double)lVar12;
              auVar20._0_8_ = 3.141592653589793 - (6.283185307179586 / (double)iVar8) * local_d0;
              auVar20._8_8_ = 0;
              dVar22 = 3.141592653589793;
              dStack_c8 = (double)lVar11;
              dVar14 = (double)func_0x0001034627b8(auVar20);
              dVar22 = dVar24 * dVar22;
              dVar14 = dVar24 * dVar14;
              local_c0 = (1.0 - (dVar34 + dVar28)) * dVar22 + (dVar33 - dVar27) * dVar14 +
                         (dVar25 + dVar32) * dVar13;
              dStack_b8 = (dVar33 + dVar27) * dVar22 + (1.0 - (dVar31 + dVar28)) * dVar14 +
                          (dVar26 - dVar29) * dVar13;
              local_b0 = (dVar25 - dVar32) * dVar22 +
                         (1.0 - (dVar31 + dVar34)) * dVar13 + (dVar26 + dVar29) * dVar14;
              (**(code **)(*plVar4 + 0x28))(plVar4,&local_c0,&local_d0);
              *(ulong *)(lVar9 + lVar12 * 8) =
                   CONCAT44((float)((auVar15._8_8_ / auVar18._8_8_) * dStack_c8),
                            (float)(dVar30 * local_d0));
              lVar12 = lVar12 + 1;
            } while (lVar10 != lVar12);
            lVar11 = lVar11 + 1;
            lVar9 = lVar9 + lVar7 * 8;
            dVar24 = dVar30;
          } while (lVar11 != lVar1);
        }
        plVar4 = local_f8;
        if (local_f8 != (long *)0x0) {
          LOAcquire();
          lVar11 = local_f8[1];
          local_f8[1] = lVar11 + -1;
          LORelease();
          if (lVar11 == 0) {
            (**(code **)(*local_f8 + 0x10))(local_f8);
            func_0x000103462194(plVar4);
          }
        }
        lVar5 = lVar5 + 1;
        local_158 = local_158 + lVar6 * 8;
      } while (lVar5 < param_1[1]);
    }
  }
  return;
}

