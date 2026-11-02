
/* WARNING: Type propagation algorithm not settling */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(long param_1,undefined8 *param_2,undefined8 param_3)

{
  char cVar1;
  undefined1 auVar2 [16];
  undefined8 uVar3;
  double dVar4;
  undefined1 *puVar5;
  undefined1 *puVar6;
  double *pdVar7;
  code *pcVar8;
  int iVar9;
  long lVar10;
  ulong uVar11;
  long *plVar12;
  long lVar13;
  undefined1 *puVar14;
  long lVar15;
  undefined8 *******pppppppuVar16;
  long lVar17;
  ulong uVar18;
  undefined8 *puVar19;
  undefined1 *puVar20;
  undefined8 *puVar21;
  double *pdVar22;
  double *pdVar23;
  undefined8 uVar24;
  double *pdVar25;
  double *pdVar26;
  undefined1 *puVar27;
  ulong uVar28;
  undefined1 *puVar29;
  float fVar30;
  float fVar31;
  float fVar32;
  undefined8 uVar33;
  double dVar34;
  float fVar35;
  double dVar36;
  undefined8 uVar37;
  double dVar38;
  float fVar39;
  double in_d3;
  undefined8 uVar40;
  double dVar41;
  double dVar42;
  double local_310;
  double dStack_308;
  double dStack_300;
  undefined4 uStack_2f8;
  undefined4 uStack_2f4;
  undefined7 local_2f0;
  undefined1 uStack_2e9;
  undefined7 uStack_2e8;
  undefined1 local_2e1;
  char local_2d9;
  undefined8 *******local_2d8;
  ulong local_2d0;
  undefined8 uStack_2c8;
  double local_2c0;
  long *plStack_2b8;
  double dStack_2b0;
  double dStack_2a8;
  long local_1b0;
  long local_1a8;
  long local_1a0;
  long local_198;
  long local_190;
  long local_188;
  undefined8 local_180;
  undefined8 uStack_178;
  undefined8 uStack_170;
  undefined8 uStack_168;
  undefined8 local_160;
  undefined8 uStack_158;
  undefined8 local_150;
  undefined8 local_140;
  undefined8 uStack_138;
  undefined8 uStack_130;
  undefined8 uStack_128;
  undefined8 local_120;
  undefined8 uStack_118;
  undefined8 local_110;
  undefined1 *local_100;
  undefined1 *local_f8;
  undefined1 *local_f0;
  double *local_e8;
  double *local_e0;
  double *local_d8;
  undefined8 local_d0;
  undefined8 uStack_c8;
  undefined8 local_c0;
  undefined8 uStack_b8;
  double dStack_b0;
  undefined8 uStack_a8;
  undefined8 local_a0;
  undefined8 local_98;
  long lStack_90;
  long local_88;
  long lStack_80;
  
  iVar9 = func_0x0001018b5e9c(param_3);
  if (*(long *)(param_1 + 0x20) == 0) {
    return;
  }
  if (*(long *)(param_1 + 0x30) == 0 || iVar9 == 0) {
    return;
  }
  lVar10 = func_0x0001034622d8();
  lVar17 = *(long *)(param_1 + 0x30);
  local_98 = *(undefined8 *)(lVar17 + 0x10);
  lStack_90 = (long)*(int *)(lVar17 + 8);
  local_88 = (long)*(int *)(lVar17 + 0xc);
  local_a0 = param_2[6];
  uStack_a8 = param_2[5];
  dVar36 = (double)param_2[4];
  uStack_c8 = param_2[1];
  local_d0 = *param_2;
  uStack_b8 = param_2[3];
  local_c0 = param_2[2];
  dStack_b0 = dVar36;
  lStack_80 = local_88;
  dVar42 = (double)(**(code **)(**(long **)(param_1 + 0x10) + 0x48))
                             (*(long **)(param_1 + 0x10),&local_d0,0);
  local_e8 = (double *)0x0;
  local_e0 = (double *)0x0;
  local_d8 = (double *)0x0;
  local_100 = (undefined1 *)0x0;
  local_f8 = (undefined1 *)0x0;
  local_f0 = (undefined1 *)0x0;
  local_e8 = (double *)func_0x000103462620(8);
  local_e0 = local_e8 + 1;
  *local_e8 = dVar42 * 0.001;
  local_d8 = local_e0;
  if (iVar9 == 1) {
    uStack_138 = param_2[1];
    local_140 = *param_2;
    uStack_128 = param_2[3];
    uStack_130 = param_2[2];
    uStack_118 = param_2[5];
    local_120 = param_2[4];
    local_110 = param_2[6];
    uVar11 = (**(code **)(**(long **)(param_1 + 0x10) + 0xa8))
                       (*(long **)(param_1 + 0x10),&local_140,1);
    lVar17 = *(long *)(param_1 + 0x20);
    dVar42 = 0.0;
    if ((uVar11 & 1) == 0) {
      dVar42 = (double)(**(code **)(**(long **)(param_1 + 0x10) + 0xa0))();
      dVar42 = dVar42 * 0.001;
    }
    *(double *)(lVar17 + 0xa8) = dVar42;
  }
  puVar6 = local_f8;
  puVar5 = local_100;
  if (local_f8 < local_f0) {
    puVar29 = local_f8 + 1;
    *local_f8 = 1;
  }
  else {
    uVar28 = (long)local_f8 - (long)local_100;
    uVar11 = uVar28 + 1;
    if ((long)uVar11 < 0) {
      func_0x000100023e9c(&local_100);
      goto LAB_101081e5c;
    }
    uVar18 = ((long)local_f0 - (long)local_100) * 2;
    if (uVar18 < uVar11 || uVar18 - uVar11 == 0) {
      uVar18 = uVar11;
    }
    if (0x3ffffffffffffffe < (ulong)((long)local_f0 - (long)local_100)) {
      uVar18 = 0x7fffffffffffffff;
    }
    if (uVar18 == 0) {
      puVar14 = (undefined1 *)0x0;
    }
    else {
      puVar14 = (undefined1 *)func_0x000103462620(uVar18);
    }
    puVar27 = puVar14 + uVar28;
    local_f0 = puVar14 + uVar18;
    puVar29 = puVar27 + 1;
    *puVar27 = 1;
    local_100 = puVar27;
    if (puVar6 != puVar5) {
      puVar20 = puVar6;
      local_100 = puVar14;
      if ((7 < uVar28) && (0x3f < (ulong)((long)puVar5 - (long)puVar14))) {
        if (uVar28 < 0x40) {
          uVar18 = 0;
        }
        else {
          uVar18 = uVar28 & 0xffffffffffffffc0;
          puVar19 = (undefined8 *)(puVar6 + -0x20);
          puVar21 = (undefined8 *)(puVar14 + ((long)puVar19 - (long)puVar5));
          uVar11 = uVar18;
          do {
            uVar33 = *puVar19;
            uVar3 = puVar19[3];
            uVar24 = puVar19[2];
            uVar40 = puVar19[-3];
            in_d3 = (double)puVar19[-4];
            uVar37 = puVar19[-1];
            dVar36 = (double)puVar19[-2];
            puVar21[1] = puVar19[1];
            *puVar21 = uVar33;
            puVar21[3] = uVar3;
            puVar21[2] = uVar24;
            puVar21[-3] = uVar40;
            puVar21[-4] = in_d3;
            puVar21[-1] = uVar37;
            puVar21[-2] = dVar36;
            puVar21 = puVar21 + -8;
            puVar19 = puVar19 + -8;
            uVar11 = uVar11 - 0x40;
          } while (uVar11 != 0);
          if (uVar28 == uVar18) goto LAB_1010817b0;
          if ((uVar28 & 0x38) == 0) {
            puVar27 = puVar27 + -uVar18;
            puVar20 = puVar6 + -uVar18;
            goto LAB_10108179c;
          }
        }
        uVar11 = uVar28 & 0xfffffffffffffff8;
        puVar20 = puVar6 + -uVar11;
        puVar27 = puVar27 + -uVar11;
        lVar15 = (long)(puVar6 + (-8 - uVar18)) - (long)puVar5;
        lVar17 = uVar18 - uVar11;
        puVar21 = (undefined8 *)(puVar6 + (-8 - uVar18));
        do {
          *(undefined8 *)(puVar14 + lVar15) = *puVar21;
          lVar15 = lVar15 + -8;
          lVar17 = lVar17 + 8;
          puVar21 = puVar21 + -1;
        } while (lVar17 != 0);
        if (uVar28 == uVar11) goto LAB_1010817b0;
      }
LAB_10108179c:
      do {
        puVar27 = puVar27 + -1;
        puVar20 = puVar20 + -1;
        *puVar27 = *puVar20;
      } while (puVar20 != puVar5);
    }
LAB_1010817b0:
    if (puVar5 != (undefined1 *)0x0) {
      local_f8 = puVar29;
      func_0x000103462518(puVar5);
    }
  }
  fVar39 = SUB84(in_d3,0);
  fVar35 = SUB84(dVar36,0);
  plVar12 = *(long **)(param_1 + 0x10);
  local_f8 = puVar29;
  if (iVar9 == 2) {
    uStack_178 = param_2[1];
    local_180 = *param_2;
    uStack_168 = param_2[3];
    uStack_170 = param_2[2];
    uStack_158 = param_2[5];
    local_160 = param_2[4];
    local_150 = param_2[6];
    dVar42 = (double)(**(code **)(*plVar12 + 0x48))(plVar12,&local_180,1);
    pdVar22 = local_e0;
    pdVar7 = local_e8;
    dVar42 = dVar42 * 0.001;
    if (local_e0 < local_d8) {
      pdVar26 = local_e0 + 1;
      *local_e0 = dVar42;
    }
    else {
      lVar15 = (long)local_e0 - (long)local_e8;
      lVar17 = lVar15 >> 3;
      uVar11 = lVar17 + 1;
      if (uVar11 >> 0x3d != 0) {
        func_0x000100023e9c(&local_e8);
        goto LAB_101081e5c;
      }
      uVar28 = (long)local_d8 - (long)local_e8 >> 2;
      if (uVar28 <= uVar11) {
        uVar28 = uVar11;
      }
      if (0x7ffffffffffffff7 < (ulong)((long)local_d8 - (long)local_e8)) {
        uVar28 = 0x1fffffffffffffff;
      }
      if (uVar28 == 0) {
        lVar13 = 0;
        local_e8 = (double *)(lVar17 * 8);
        local_d8 = (double *)0x0;
        *local_e8 = dVar42;
      }
      else {
        if (uVar28 >> 0x3d != 0) {
          func_0x00010002db08();
          goto LAB_101081e5c;
        }
        lVar13 = func_0x000103462620(uVar28 << 3);
        local_e8 = (double *)(lVar13 + lVar17 * 8);
        local_d8 = (double *)(lVar13 + uVar28 * 8);
        *local_e8 = dVar42;
      }
      pdVar26 = local_e8 + 1;
      if ((long)pdVar22 - (long)pdVar7 != 0) {
        uVar11 = ((long)pdVar22 - (long)pdVar7) - 8;
        pdVar25 = pdVar22;
        if ((0x57 < uVar11) && (0x3f < (ulong)((long)pdVar22 - (lVar15 + lVar13)))) {
          uVar11 = (uVar11 >> 3) + 1;
          uVar18 = uVar11 & 0x3ffffffffffffff8;
          pdVar25 = pdVar22 + -uVar18;
          local_e8 = local_e8 + -uVar18;
          pdVar22 = pdVar22 + -4;
          pdVar23 = (double *)(lVar13 + lVar17 * 8 + -0x20);
          uVar28 = uVar18;
          do {
            dVar34 = *pdVar22;
            dVar4 = pdVar22[3];
            dVar42 = pdVar22[2];
            dVar41 = pdVar22[-3];
            in_d3 = pdVar22[-4];
            dVar38 = pdVar22[-1];
            dVar36 = pdVar22[-2];
            pdVar23[1] = pdVar22[1];
            *pdVar23 = dVar34;
            pdVar23[3] = dVar4;
            pdVar23[2] = dVar42;
            pdVar23[-3] = dVar41;
            pdVar23[-4] = in_d3;
            pdVar23[-1] = dVar38;
            pdVar23[-2] = dVar36;
            pdVar22 = pdVar22 + -8;
            pdVar23 = pdVar23 + -8;
            uVar28 = uVar28 - 8;
          } while (uVar28 != 0);
          if (uVar11 == uVar18) goto LAB_101081934;
        }
        do {
          pdVar25 = pdVar25 + -1;
          local_e8 = local_e8 + -1;
          *local_e8 = *pdVar25;
        } while (pdVar25 != pdVar7);
      }
LAB_101081934:
      if (pdVar7 != (double *)0x0) {
        local_e0 = pdVar26;
        func_0x000103462518(pdVar7);
      }
    }
    puVar6 = local_f8;
    puVar5 = local_100;
    fVar39 = SUB84(in_d3,0);
    fVar35 = SUB84(dVar36,0);
    local_e0 = pdVar26;
    if (local_f8 < local_f0) {
      puVar27 = local_f8 + 1;
      *local_f8 = 1;
    }
    else {
      uVar28 = (long)local_f8 - (long)local_100;
      uVar11 = uVar28 + 1;
      if ((long)uVar11 < 0) {
        func_0x000100023e9c(&local_100);
        goto LAB_101081e5c;
      }
      uVar18 = ((long)local_f0 - (long)local_100) * 2;
      if (uVar18 < uVar11 || uVar18 - uVar11 == 0) {
        uVar18 = uVar11;
      }
      if (0x3ffffffffffffffe < (ulong)((long)local_f0 - (long)local_100)) {
        uVar18 = 0x7fffffffffffffff;
      }
      if (uVar18 == 0) {
        puVar14 = (undefined1 *)0x0;
      }
      else {
        puVar14 = (undefined1 *)func_0x000103462620(uVar18);
      }
      fVar39 = SUB84(in_d3,0);
      fVar35 = SUB84(dVar36,0);
      local_100 = puVar14 + uVar28;
      local_f0 = puVar14 + uVar18;
      puVar27 = local_100 + 1;
      *local_100 = 1;
      if (puVar6 != puVar5) {
        puVar29 = puVar6;
        if ((uVar28 < 8) || ((ulong)((long)puVar5 - (long)puVar14) < 0x40)) {
LAB_101081a84:
          do {
            local_100 = local_100 + -1;
            puVar29 = puVar29 + -1;
            *local_100 = *puVar29;
          } while (puVar29 != puVar5);
        }
        else if (uVar28 < 0x40) {
          uVar18 = 0;
LAB_101081a38:
          uVar11 = uVar28 & 0xfffffffffffffff8;
          puVar29 = puVar6 + -uVar11;
          local_100 = local_100 + -uVar11;
          lVar15 = (long)(puVar6 + (-8 - uVar18)) - (long)puVar5;
          lVar17 = uVar18 - uVar11;
          puVar21 = (undefined8 *)(puVar6 + (-8 - uVar18));
          do {
            *(undefined8 *)(puVar14 + lVar15) = *puVar21;
            lVar15 = lVar15 + -8;
            lVar17 = lVar17 + 8;
            puVar21 = puVar21 + -1;
          } while (lVar17 != 0);
          if (uVar28 != uVar11) goto LAB_101081a84;
        }
        else {
          uVar18 = uVar28 & 0xffffffffffffffc0;
          puVar19 = (undefined8 *)(puVar6 + -0x20);
          puVar21 = (undefined8 *)(puVar14 + ((long)puVar19 - (long)puVar5));
          uVar11 = uVar18;
          do {
            uVar33 = *puVar19;
            uVar3 = puVar19[3];
            uVar24 = puVar19[2];
            uVar40 = puVar19[-3];
            in_d3 = (double)puVar19[-4];
            uVar37 = puVar19[-1];
            dVar36 = (double)puVar19[-2];
            puVar21[1] = puVar19[1];
            *puVar21 = uVar33;
            puVar21[3] = uVar3;
            puVar21[2] = uVar24;
            puVar21[-3] = uVar40;
            puVar21[-4] = in_d3;
            puVar21[-1] = uVar37;
            puVar21[-2] = dVar36;
            puVar21 = puVar21 + -8;
            puVar19 = puVar19 + -8;
            uVar11 = uVar11 - 0x40;
          } while (uVar11 != 0);
          if (uVar28 != uVar18) {
            if ((uVar28 & 0x38) == 0) {
              local_100 = local_100 + -uVar18;
              puVar29 = puVar6 + -uVar18;
              goto LAB_101081a84;
            }
            goto LAB_101081a38;
          }
        }
        fVar39 = SUB84(in_d3,0);
        fVar35 = SUB84(dVar36,0);
        local_100 = puVar14;
      }
      if (puVar5 != (undefined1 *)0x0) {
        local_f8 = puVar27;
        func_0x000103462518(puVar5);
      }
    }
    plVar12 = *(long **)(param_1 + 0x10);
    local_f8 = puVar27;
  }
  iVar9 = (**(code **)(*plVar12 + 0x78))();
  pdVar7 = local_e8;
  if (iVar9 == 0) {
    (**(code **)(**(long **)(param_1 + 0x10) + 0x98))(&local_2c0);
    plVar12 = plStack_2b8;
    dVar42 = local_2c0;
    if (plStack_2b8 != (long *)0x0) {
      LOAcquire();
      lVar17 = plStack_2b8[1];
      plStack_2b8[1] = lVar17 + -1;
      LORelease();
      if (lVar17 == 0) {
        (**(code **)(*plStack_2b8 + 0x10))(plStack_2b8);
        func_0x000103462194(plVar12);
      }
    }
    if (dVar42 != 0.0) {
      func_0x000101b55b84(&local_2c0,*(undefined8 *)(param_1 + 0x20),&local_e8,&local_100,&local_98)
      ;
      dStack_308 = dStack_2a8;
      local_310 = dStack_2b0;
      uStack_2f8 = SUB84(plStack_2b8,0);
      uStack_2f4 = (undefined4)((ulong)plStack_2b8 >> 0x20);
      dStack_300 = local_2c0;
    }
  }
  else {
    uVar24 = *(undefined8 *)(param_1 + 0x20);
    local_190 = 0;
    local_188 = 0;
    local_198 = 0;
    lVar17 = (long)local_e0 - (long)local_e8;
    if (lVar17 != 0) {
      if (lVar17 < 0) {
        func_0x000100023e9c(&local_198);
        goto LAB_101081e5c;
      }
      local_198 = func_0x000103462620(lVar17);
      lVar15 = local_198 + (lVar17 >> 3) * 8;
      local_190 = local_198;
      local_188 = lVar15;
      func_0x000103463430(local_198,pdVar7,lVar17);
      local_190 = lVar15;
    }
    puVar5 = local_100;
    local_1b0 = 0;
    local_1a8 = 0;
    local_1a0 = 0;
    lVar17 = (long)local_f8 - (long)local_100;
    if (lVar17 != 0) {
      if (lVar17 < 0) {
        func_0x000100023e9c(&local_1b0);
        goto LAB_101081e5c;
      }
      local_1b0 = func_0x000103462620(lVar17);
      lVar15 = local_1b0 + lVar17;
      local_1a8 = local_1b0;
      local_1a0 = lVar15;
      func_0x000103463430(local_1b0,puVar5,lVar17);
      local_1a8 = lVar15;
    }
    func_0x000101b55edc(&local_2c0,uVar24,&local_198,&local_1b0,&local_98);
    dStack_308 = dStack_2a8;
    local_310 = dStack_2b0;
    uStack_2f8 = SUB84(plStack_2b8,0);
    uStack_2f4 = (undefined4)((ulong)plStack_2b8 >> 0x20);
    dStack_300 = local_2c0;
    if (local_1b0 != 0) {
      local_1a8 = local_1b0;
      func_0x000103462518();
    }
    if (local_198 != 0) {
      local_190 = local_198;
      func_0x000103462518();
    }
  }
  lVar17 = 0x103d1e53f;
  lVar15 = func_0x000103463bb0(0x103d1e53f,0x2f);
  if (lVar15 != 0) {
    lVar17 = lVar15 + 1;
  }
  uVar11 = func_0x000103463b5c(lVar17);
  if (0x7ffffffffffffff7 < uVar11) {
    func_0x000100016eb8(&local_2d8);
LAB_101081e5c:
                    /* WARNING: Does not return */
    pcVar8 = (code *)SoftwareBreakpoint(1,0x101081e60);
    (*pcVar8)();
  }
  if (uVar11 < 0x17) {
    uStack_2c8 = CONCAT17((char)uVar11,(undefined7)uStack_2c8);
    pppppppuVar16 = &local_2d8;
    if (uVar11 == 0) goto LAB_101081c80;
  }
  else {
    uVar28 = (uVar11 & 0xfffffffffffffff8) + 8;
    if ((uVar11 | 7) != 0x17) {
      uVar28 = uVar11 | 7;
    }
    pppppppuVar16 = (undefined8 *******)func_0x000103462620(uVar28 + 1);
    uStack_2c8 = uVar28 + 1 | 0x8000000000000000;
    local_2d8 = pppppppuVar16;
    local_2d0 = uVar11;
  }
  func_0x00010346343c(pppppppuVar16,lVar17,uVar11);
LAB_101081c80:
  *(undefined1 *)((long)pppppppuVar16 + uVar11) = 0;
  local_2d9 = '\x0f';
  local_2f0 = _DAT_103d1e64f;
  uStack_2e9 = DAT_103d1e656;
  uStack_2e8 = uRam0000000103d1e657;
  local_2e1 = 0;
  func_0x00010002b8a0(&local_2c0,&local_2d8,&local_2f0,0xd2,1);
  func_0x000100016bb0(&local_2c0,0x103d1e65f,0x24);
  lVar17 = func_0x0001034622d8();
  func_0x000103461fcc(SUB84((double)((lVar17 - lVar10) / 1000) / 1000.0,0),&local_2c0);
  func_0x00010002bfd0(&local_2c0);
  if (local_2d9 < '\0') {
    func_0x000103462518(CONCAT17(uStack_2e9,local_2f0));
  }
  if ((long)uStack_2c8 < 0) {
    func_0x000103462518(local_2d8);
    cVar1 = *(char *)(param_1 + 0x60);
  }
  else {
    cVar1 = *(char *)(param_1 + 0x60);
  }
  if (cVar1 == '\0') {
    *(undefined1 *)(param_1 + 0x60) = 1;
  }
  auVar2._8_4_ = uStack_2f8;
  auVar2._0_8_ = dStack_300;
  *(ulong *)(param_1 + 0x48) = CONCAT44(uStack_2f4,uStack_2f8);
  *(double *)(param_1 + 0x40) = dStack_300;
  *(double *)(param_1 + 0x58) = dStack_308;
  *(double *)(param_1 + 0x50) = local_310;
  auVar2._12_4_ = uStack_2f4;
  plStack_2b8 = (long *)CONCAT44((float)dStack_308,(float)local_310);
  local_2c0 = (double)CONCAT44((float)auVar2._8_8_,(float)dStack_300);
  fVar30 = (float)func_0x00010187785c(&local_2c0);
  fVar32 = SUB84(local_310,0);
  fVar31 = fVar30 * fVar30 + fVar32 * fVar32 + fVar35 * fVar35 + fVar39 * fVar39;
  *(ulong *)(param_1 + 0x78) = CONCAT44(fVar39 / fVar31,-fVar35 / fVar31);
  *(ulong *)(param_1 + 0x70) = CONCAT44(-fVar32 / fVar31,-fVar30 / fVar31);
  if (*(char *)(param_1 + 0x80) == '\0') {
    *(undefined1 *)(param_1 + 0x80) = 1;
  }
  if (local_100 != (undefined1 *)0x0) {
    local_f8 = local_100;
    func_0x000103462518();
  }
  if (local_e8 != (double *)0x0) {
    local_e0 = local_e8;
    func_0x000103462518();
  }
  return;
}

