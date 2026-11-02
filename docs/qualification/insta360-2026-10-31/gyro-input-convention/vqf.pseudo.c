
/* WARNING: Type propagation algorithm not settling */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

undefined8 entry(long param_1,long *param_2)

{
  uint uVar1;
  ulong uVar2;
  undefined8 uVar3;
  bool bVar4;
  undefined8 *puVar5;
  undefined8 uVar6;
  undefined8 uVar7;
  double *pdVar8;
  code *pcVar9;
  int iVar10;
  long lVar11;
  undefined8 *puVar12;
  double **ppdVar13;
  long *plVar14;
  int iVar15;
  int iVar16;
  long lVar17;
  double *pdVar18;
  ulong uVar19;
  long lVar20;
  double *pdVar21;
  double *pdVar22;
  undefined8 *puVar23;
  double *pdVar24;
  long lVar25;
  long lVar26;
  undefined8 *puVar27;
  ulong uVar28;
  double *pdVar29;
  ulong uVar30;
  undefined8 *puVar31;
  uint uVar32;
  uint uVar33;
  ulong uVar34;
  double dVar35;
  double dVar36;
  double dVar37;
  undefined1 auVar38 [16];
  undefined1 auVar39 [16];
  undefined1 auVar40 [16];
  undefined1 auVar41 [16];
  undefined1 auVar42 [16];
  undefined1 auVar43 [16];
  undefined1 auVar44 [16];
  undefined1 auVar45 [16];
  undefined1 auVar46 [16];
  undefined1 auVar47 [16];
  undefined1 auVar48 [16];
  undefined1 auVar49 [16];
  undefined1 auVar50 [16];
  undefined1 auVar51 [16];
  undefined1 auVar52 [16];
  double dVar53;
  undefined1 auVar54 [16];
  undefined1 auVar55 [16];
  undefined1 auVar56 [16];
  undefined1 auVar57 [16];
  undefined1 auVar58 [16];
  undefined1 auVar59 [16];
  undefined1 auVar60 [16];
  undefined1 auVar61 [16];
  undefined1 auVar62 [16];
  undefined1 auVar63 [16];
  double dVar64;
  double dVar65;
  double dVar66;
  double dVar67;
  double dVar68;
  double dVar69;
  double dVar70;
  double dVar71;
  double dVar72;
  double dVar73;
  double dVar74;
  double dVar75;
  double dVar76;
  double dVar77;
  double dVar78;
  double dStack_a48;
  double local_a00;
  double local_9f0;
  double dStack_9e8;
  double local_9e0;
  double dStack_9d8;
  double *local_9c8;
  double *local_9c0;
  double *local_9b8;
  double *local_9b0;
  double *local_9a8;
  double *local_9a0;
  double *local_998;
  double *local_990;
  double *pdStack_988;
  double local_980;
  double dStack_978;
  double local_970;
  double dStack_968;
  double *local_960;
  double *local_958;
  double *local_950;
  undefined8 *local_948;
  undefined8 *local_940;
  undefined8 *local_938;
  undefined1 auStack_930 [264];
  undefined8 local_828;
  undefined8 local_820;
  undefined8 local_818;
  double local_4b0;
  double dStack_4a8;
  double local_4a0;
  ulong local_428 [5];
  undefined8 local_400;
  double *local_370;
  double *local_368;
  double *local_360;
  double *local_358;
  double *local_350;
  double *local_348;
  double *local_340;
  double *local_338;
  double *local_330;
  double *local_328;
  double *local_320;
  double *local_318;
  double local_310;
  double dStack_308;
  double local_300;
  double local_2f8;
  double dStack_2f0;
  double local_2e8;
  double local_2e0;
  double dStack_2d8;
  double local_2d0;
  double local_2c8;
  double local_2c0;
  double dStack_2b8;
  double local_2b0;
  double local_2a8;
  double dStack_2a0;
  double local_298;
  double local_290;
  double dStack_288;
  double local_280;
  double local_278;
  undefined1 auStack_268 [24];
  undefined1 auStack_250 [24];
  undefined1 auStack_238 [16];
  undefined1 auStack_228 [16];
  undefined1 auStack_218 [216];
  double local_140;
  double dStack_138;
  double local_130;
  double local_120;
  double dStack_118;
  double local_110;
  double dStack_108;
  double local_100;
  double dStack_f8;
  double local_f0;
  double local_e8;
  double local_e0;
  double local_d8;
  double local_d0;
  double local_c8;
  double local_c0;
  long local_b8;
  
  local_b8 = *_DAT_103fed798;
  func_0x0001027b7dcc(param_1,(int)((ulong)(param_2[1] - *param_2) >> 4) * -0x33333333);
  uVar19 = 0;
  if ((ulong)*(uint *)(param_1 + 0x338) != 0) {
    uVar19 = (ulong)((param_2[1] - *param_2 >> 4) * -0x3333333333333333) /
             (ulong)*(uint *)(param_1 + 0x338);
  }
  uVar34 = *(ulong *)(param_1 + 0xa0);
  if (uVar34 < uVar19) {
    if (uVar19 >> 0x3b == 0) {
      lVar11 = func_0x000103462620(uVar19 << 5);
      lVar17 = *(long *)(param_1 + 0xa8);
      if (lVar17 != 0) {
        lVar25 = *(long *)(param_1 + 0x98);
        uVar28 = *(ulong *)(param_1 + 0xb0);
        puVar12 = (undefined8 *)(lVar11 + 0x10);
        do {
          uVar30 = 0;
          if (uVar34 != 0) {
            uVar30 = uVar28 / uVar34;
          }
          puVar31 = (undefined8 *)(lVar25 + (uVar28 - uVar30 * uVar34) * 0x20);
          uVar3 = *puVar31;
          puVar12[-1] = puVar31[1];
          puVar12[-2] = uVar3;
          auVar38 = *(undefined1 (*) [16])(puVar31 + 2);
          puVar12[1] = auVar38._8_8_;
          *puVar12 = auVar38._0_8_;
          uVar28 = uVar28 + 1;
          lVar17 = lVar17 + -1;
          puVar12 = puVar12 + 4;
        } while (lVar17 != 0);
      }
      if (uVar34 != 0) {
        func_0x000103462518(*(undefined8 *)(param_1 + 0x98));
      }
      *(long *)(param_1 + 0x98) = lVar11;
      *(ulong *)(param_1 + 0xa0) = uVar19;
      *(undefined8 *)(param_1 + 0xb0) = 0;
      goto LAB_1027b9158;
    }
LAB_1027bc230:
    func_0x00010002db08();
LAB_1027bc234:
    plVar14 = (long *)func_0x0001034626a4(0x10);
    func_0x000103461c18(plVar14,0x103d6706d);
    *plVar14 = _DAT_103fec598 + 0x10;
    func_0x000103462734(plVar14,_DAT_103fec330,_DAT_103febb78);
  }
  else {
LAB_1027b9158:
    uVar34 = *(ulong *)(param_1 + 200);
    if (uVar34 < uVar19) {
      if (uVar19 >> 0x3d != 0) goto LAB_1027bc230;
      puVar12 = (undefined8 *)func_0x000103462620(uVar19 << 3);
      lVar17 = *(long *)(param_1 + 0xd0);
      if (lVar17 != 0) {
        lVar11 = *(long *)(param_1 + 0xc0);
        uVar28 = *(ulong *)(param_1 + 0xd8);
        puVar31 = puVar12;
        do {
          uVar30 = 0;
          if (uVar34 != 0) {
            uVar30 = uVar28 / uVar34;
          }
          *puVar31 = *(undefined8 *)(lVar11 + (uVar28 - uVar30 * uVar34) * 8);
          uVar28 = uVar28 + 1;
          lVar17 = lVar17 + -1;
          puVar31 = puVar31 + 1;
        } while (lVar17 != 0);
      }
      if (uVar34 != 0) {
        func_0x000103462518(*(undefined8 *)(param_1 + 0xc0));
      }
      *(undefined8 **)(param_1 + 0xc0) = puVar12;
      *(ulong *)(param_1 + 200) = uVar19;
      *(undefined8 *)(param_1 + 0xd8) = 0;
    }
    pdVar18 = (double *)*param_2;
    local_310 = *pdVar18;
    dStack_308 = pdVar18[1];
    local_300 = pdVar18[2];
    local_2f8 = pdVar18[3];
    dStack_2f0 = pdVar18[4];
    local_2e8 = pdVar18[5];
    local_2e0 = pdVar18[6];
    dStack_2d8 = pdVar18[7];
    local_2d0 = pdVar18[8];
    local_2c8 = pdVar18[9];
    local_2c0 = local_310;
    dStack_2b8 = dStack_308;
    local_2b0 = local_300;
    local_2a8 = local_2f8;
    dStack_2a0 = dStack_2f0;
    local_298 = local_2e8;
    local_290 = local_2e0;
    dStack_288 = dStack_2d8;
    local_280 = local_2d0;
    local_278 = local_2c8;
    func_0x0001027b0f4c(&local_310,*(undefined4 *)(param_1 + 0x58));
    uVar19 = (param_2[1] - *param_2 >> 4) * -0x3333333333333333;
    dVar67 = (*(double *)(param_2[1] + -8) - *(double *)(*param_2 + 0x48)) / (double)(uVar19 - 1);
    uVar32 = 10;
    if (0.0015 <= dVar67) {
      uVar32 = 5;
    }
    uVar28 = (ulong)uVar32;
    local_320 = (double *)0x0;
    local_328 = (double *)0x0;
    uVar34 = 0;
    if (uVar28 != 0) {
      uVar34 = uVar19 / uVar28;
    }
    local_318 = (double *)0x0;
    local_338 = (double *)0x0;
    local_340 = (double *)0x0;
    local_330 = (double *)0x0;
    lVar17 = uVar34 << 0x20;
    iVar15 = (int)uVar34;
    lVar11 = (long)iVar15;
    if (lVar17 == 0) {
      local_350 = (double *)0x0;
      local_358 = (double *)0x0;
      local_348 = (double *)0x0;
      local_368 = (double *)0x0;
      local_360 = (double *)0x0;
LAB_1027b9374:
      dVar35 = (double)uVar32;
      local_370 = local_368;
      if (0 < iVar15) {
        lVar25 = 0;
        iVar10 = 0;
        do {
          iVar10 = iVar10 + 1;
          dStack_9e8 = 0.0;
          local_9f0 = 0.0;
          dStack_9d8 = 0.0;
          local_9e0 = 0.0;
          dVar73 = 0.0;
          dVar71 = 0.0;
          uVar19 = uVar28;
          lVar26 = lVar25;
          do {
            pdVar18 = (double *)(*param_2 + lVar26);
            local_2c0 = *pdVar18;
            dStack_2b8 = pdVar18[1];
            local_2b0 = pdVar18[2];
            local_2a8 = pdVar18[3];
            dStack_2a0 = pdVar18[4];
            local_298 = pdVar18[5];
            local_290 = pdVar18[6];
            dStack_288 = pdVar18[7];
            local_280 = pdVar18[8];
            local_278 = pdVar18[9];
            func_0x0001027b0f4c(&local_2c0,*(undefined4 *)(param_1 + 0x58));
            pdVar18 = local_350;
            pdVar29 = local_358;
            dVar36 = local_2a8;
            if (52.0 < ABS(local_2a8 - local_2f8)) {
              dVar36 = local_2f8;
            }
            local_2f8 = dVar36;
            dVar36 = dStack_2a0;
            if (52.0 < ABS(dStack_2a0 - dStack_2f0)) {
              dVar36 = dStack_2f0;
            }
            dStack_2f0 = dVar36;
            dVar36 = local_298;
            if (52.0 < ABS(local_298 - local_2e8)) {
              dVar36 = local_2e8;
            }
            local_2e8 = dVar36;
            dVar36 = local_2c0;
            if (45.0 < ABS(local_2c0 - local_310)) {
              dVar36 = local_310;
            }
            local_310 = dVar36;
            dVar36 = dStack_2b8;
            if (45.0 < ABS(dStack_2b8 - dStack_308)) {
              dVar36 = dStack_308;
            }
            dStack_308 = dVar36;
            if (45.0 < ABS(local_2b0 - local_300)) {
              local_2b0 = local_300;
            }
            dStack_2d8 = dStack_288;
            local_2e0 = local_290;
            local_2c8 = local_278;
            local_2d0 = local_280;
            local_9f0 = local_9f0 - dStack_2f0;
            dStack_9e8 = dStack_9e8 + local_2f8 + 0.0;
            dVar73 = dVar73 + local_2e8 + 0.0;
            local_9e0 = local_9e0 + dStack_308 * 9.8;
            dStack_9d8 = dStack_9d8 + -local_310 * 9.8;
            dVar71 = dVar71 + local_2b0 * -9.8;
            lVar26 = lVar26 + 0x50;
            uVar19 = uVar19 - 1;
            local_300 = local_2b0;
          } while (uVar19 != 0);
          local_2c0 = local_310;
          dStack_2b8 = dStack_308;
          local_2a8 = local_2f8;
          dStack_2a0 = dStack_2f0;
          local_298 = local_2e8;
          if (local_348 <= local_350) {
            lVar26 = (long)local_350 - (long)local_358 >> 3;
            uVar19 = lVar26 * -0x5555555555555555 + 1;
            if (uVar19 < 0xaaaaaaaaaaaaaab) {
              lVar20 = (long)local_348 - (long)local_358 >> 3;
              uVar34 = lVar20 * 0x5555555555555556;
              if (uVar34 < uVar19 || uVar34 - uVar19 == 0) {
                uVar34 = uVar19;
              }
              if (0x555555555555554 < (ulong)(lVar20 * -0x5555555555555555)) {
                uVar34 = 0xaaaaaaaaaaaaaaa;
              }
              if (uVar34 == 0) {
                lVar20 = 0;
LAB_1027b96e0:
                pdVar21 = (double *)(lVar20 + lVar26 * 8);
                pdVar21[1] = dStack_9e8 / dVar35;
                *pdVar21 = local_9f0 / dVar35;
                pdVar21[2] = dVar73 / dVar35;
                pdVar22 = pdVar21;
                pdVar24 = pdVar18;
                for (; pdVar18 != pdVar29; pdVar18 = pdVar18 + -3) {
                  auVar38 = *(undefined1 (*) [16])(pdVar18 + -3);
                  pdVar22[-1] = pdVar18[-1];
                  pdVar22[-2] = auVar38._8_8_;
                  pdVar22[-3] = auVar38._0_8_;
                  pdVar22 = pdVar22 + -3;
                  pdVar24 = local_358;
                }
                local_348 = (double *)(lVar20 + uVar34 * 0x18);
                local_358 = pdVar22;
                if (pdVar24 != (double *)0x0) {
                  func_0x000103462518(pdVar24);
                }
                goto LAB_1027b974c;
              }
              if (uVar34 < 0xaaaaaaaaaaaaaab) {
                lVar20 = func_0x000103462620(uVar34 * 0x18);
                goto LAB_1027b96e0;
              }
LAB_1027bc190:
              func_0x00010002db08();
            }
            else {
              ppdVar13 = &local_358;
LAB_1027bc1bc:
              func_0x000100023e9c(ppdVar13);
            }
            goto LAB_1027bc2a8;
          }
          local_350[1] = dStack_9e8 / dVar35;
          *local_350 = local_9f0 / dVar35;
          local_350[2] = dVar73 / dVar35;
          pdVar21 = local_350;
LAB_1027b974c:
          pdVar18 = local_368;
          pdVar29 = local_370;
          local_350 = pdVar21 + 3;
          if (local_368 < local_360) {
            local_368[1] = dStack_9d8 / dVar35;
            *local_368 = local_9e0 / dVar35;
            local_368[2] = dVar71 / dVar35;
            pdVar21 = local_368;
          }
          else {
            lVar26 = (long)local_368 - (long)local_370 >> 3;
            uVar19 = lVar26 * -0x5555555555555555 + 1;
            if (0xaaaaaaaaaaaaaaa < uVar19) {
              ppdVar13 = &local_370;
              goto LAB_1027bc1bc;
            }
            lVar20 = (long)local_360 - (long)local_370 >> 3;
            uVar34 = lVar20 * 0x5555555555555556;
            if (uVar34 < uVar19 || uVar34 - uVar19 == 0) {
              uVar34 = uVar19;
            }
            if (0x555555555555554 < (ulong)(lVar20 * -0x5555555555555555)) {
              uVar34 = 0xaaaaaaaaaaaaaaa;
            }
            if (uVar34 == 0) {
              lVar20 = 0;
            }
            else {
              if (0xaaaaaaaaaaaaaaa < uVar34) goto LAB_1027bc190;
              lVar20 = func_0x000103462620(uVar34 * 0x18);
            }
            pdVar21 = (double *)(lVar20 + lVar26 * 8);
            pdVar21[1] = dStack_9d8 / dVar35;
            *pdVar21 = local_9e0 / dVar35;
            pdVar21[2] = dVar71 / dVar35;
            pdVar22 = pdVar21;
            pdVar24 = pdVar18;
            for (; pdVar18 != pdVar29; pdVar18 = pdVar18 + -3) {
              auVar38 = *(undefined1 (*) [16])(pdVar18 + -3);
              pdVar22[-1] = pdVar18[-1];
              pdVar22[-2] = auVar38._8_8_;
              pdVar22[-3] = auVar38._0_8_;
              pdVar22 = pdVar22 + -3;
              pdVar24 = local_370;
            }
            local_360 = (double *)(lVar20 + uVar34 * 0x18);
            local_370 = pdVar22;
            if (pdVar24 != (double *)0x0) {
              func_0x000103462518(pdVar24);
            }
          }
          local_368 = pdVar21 + 3;
          lVar25 = lVar25 + uVar28 * 0x50;
        } while (iVar10 != iVar15);
      }
      func_0x0001027d1ad8(local_428);
      auVar38._0_8_ = dVar67 * dVar35;
      auVar38._8_8_ = 0;
      local_428[0] = 0x4024000000000000;
      local_400 = 0x4000000000000000;
      func_0x0001027d1fcc(auVar38,ZEXT816(0xbff0000000000000),0xbff0000000000000,auStack_930,
                          local_428);
      dStack_118 = 0.0;
      local_120 = 0.0;
      dStack_108 = 0.0;
      local_110 = 0.0;
      if (local_350 != local_358) {
        lVar25 = 0;
        uVar19 = 0;
        do {
          pdVar18 = (double *)((long)local_358 + lVar25);
          local_d0 = *pdVar18;
          local_c8 = pdVar18[1];
          local_c0 = pdVar18[2];
          pdVar18 = (double *)((long)local_370 + lVar25);
          local_e8 = *pdVar18;
          local_e0 = pdVar18[1];
          local_d8 = pdVar18[2];
          func_0x0001027d3cb0(auStack_930,&local_d0,&local_e8);
          uVar19 = uVar19 + 1;
          uVar34 = ((long)local_350 - (long)local_358 >> 3) * -0x5555555555555555;
          lVar25 = lVar25 + 0x18;
        } while (uVar19 <= uVar34 && uVar34 - uVar19 != 0);
      }
      func_0x0001027d20e4(auStack_930);
      uVar33 = (int)((ulong)((long)local_350 - (long)local_358) >> 3) * -0x55555555;
      while (uVar33 = uVar33 - 1, -1 < (int)uVar33) {
        pdVar18 = local_358 + (ulong)uVar33 * 3;
        local_d0 = -*pdVar18;
        local_c8 = -pdVar18[1];
        local_c0 = -pdVar18[2];
        pdVar18 = local_370 + (ulong)uVar33 * 3;
        local_e8 = *pdVar18;
        local_e0 = pdVar18[1];
        local_d8 = pdVar18[2];
        func_0x0001027d3cb0(auStack_930,&local_d0,&local_e8);
      }
      func_0x0001027d20e4(auStack_930);
      local_948 = (undefined8 *)0x0;
      local_940 = (undefined8 *)0x0;
      local_938 = (undefined8 *)0x0;
      local_960 = (double *)0x0;
      local_958 = (double *)0x0;
      local_950 = (double *)0x0;
      if (lVar17 != 0) {
        if (lVar17 < 0) {
          func_0x000100023e9c(&local_948);
          goto LAB_1027bc2a8;
        }
        local_948 = (undefined8 *)func_0x000103462620(lVar11 * 0x18);
        local_938 = local_948 + (long)iVar15 * 3;
        local_940 = local_948;
        local_958 = (double *)func_0x000103462620(lVar11 * 0x18);
        local_950 = local_958 + (long)iVar15 * 3;
      }
      local_960 = local_958;
      if (local_350 != local_358) {
        uVar19 = 0;
        do {
          pdVar18 = local_358 + uVar19 * 3;
          local_d0 = *pdVar18;
          local_c8 = pdVar18[1];
          local_c0 = pdVar18[2];
          pdVar18 = local_370 + uVar19 * 3;
          local_e8 = *pdVar18;
          local_e0 = pdVar18[1];
          local_d8 = pdVar18[2];
          func_0x0001027d3cb0(auStack_930,&local_d0,&local_e8);
          func_0x0001027d3cdc(auStack_930,&local_100);
          lVar17 = func_0x0001027d3e40(auStack_930);
          dVar70 = local_f0;
          dVar37 = dStack_f8;
          dVar36 = local_100;
          pdVar18 = local_320;
          pdVar29 = local_328;
          dVar73 = local_4a0;
          dVar71 = dStack_4a8;
          dVar67 = local_4b0;
          uVar7 = local_818;
          uVar6 = local_820;
          uVar3 = local_828;
          bVar4 = true;
          if (10.6 <= SQRT(local_4a0 * local_4a0 + local_4b0 * local_4b0 + dStack_4a8 * dStack_4a8))
          goto LAB_1027ba0f8;
          dVar64 = *(double *)(lVar17 + 0xd0);
          dVar77 = *(double *)(lVar17 + 0xd8);
          dVar68 = *(double *)(lVar17 + 0xe0);
          dVar65 = *(double *)(lVar17 + 0xe8);
          dVar78 = *(double *)(lVar17 + 0xf0);
          dVar69 = *(double *)(lVar17 + 0xf8);
          dVar74 = *(double *)(lVar17 + 0x100);
          dVar53 = *(double *)(lVar17 + 0x108);
          dVar76 = *(double *)(lVar17 + 0x110);
          dVar66 = -(dVar69 * dVar53) + dVar76 * dVar78;
          dStack_118 = -(dVar76 * dVar77) + dVar68 * dVar53;
          dVar72 = -(dVar68 * dVar78) + dVar69 * dVar77;
          dVar75 = 1.0 / (dVar72 * dVar74 + dVar64 * dVar66 + dVar65 * dStack_118);
          dStack_118 = dStack_118 * dVar75;
          local_110 = dVar75 * (-(dVar74 * dVar68) + dVar64 * dVar76);
          local_120 = (-(dVar78 * dVar74) + dVar53 * dVar65) * dVar75;
          dStack_108 = dVar75 * (-(dVar53 * dVar64) + dVar77 * dVar74);
          if (local_318 <= local_320) {
            lVar17 = (long)local_320 - (long)local_328 >> 3;
            uVar34 = lVar17 * -0x5555555555555555 + 1;
            if (uVar34 < 0xaaaaaaaaaaaaaab) {
              lVar11 = (long)local_318 - (long)local_328 >> 3;
              uVar28 = lVar11 * 0x5555555555555556;
              if (uVar28 < uVar34 || uVar28 - uVar34 == 0) {
                uVar28 = uVar34;
              }
              if (0x555555555555554 < (ulong)(lVar11 * -0x5555555555555555)) {
                uVar28 = 0xaaaaaaaaaaaaaaa;
              }
              pdVar21 = pdVar18;
              if (uVar28 == 0) {
                lVar11 = 0;
                pdVar22 = (double *)(lVar17 * 8);
                pdVar22[1] = dStack_f8;
                *pdVar22 = local_100;
                pdVar22[2] = local_f0;
                pdVar8 = pdVar22;
                pdVar24 = local_328;
joined_r0x0001027b9cdc:
                for (; local_328 = pdVar8, pdVar18 != pdVar29; pdVar18 = pdVar18 + -3) {
                  auVar39 = *(undefined1 (*) [16])(pdVar18 + -3);
                  local_328[-1] = pdVar18[-1];
                  local_328[-2] = auVar39._8_8_;
                  local_328[-3] = auVar39._0_8_;
                  pdVar21 = pdVar24;
                  pdVar8 = local_328 + -3;
                }
                local_318 = (double *)(lVar11 + uVar28 * 0x18);
                pdVar22 = pdVar22 + 3;
                if (pdVar21 != (double *)0x0) {
                  local_320 = pdVar22;
                  func_0x000103462518(pdVar21);
                }
                goto LAB_1027b9d38;
              }
              if (uVar28 < 0xaaaaaaaaaaaaaab) {
                lVar11 = func_0x000103462620(uVar28 * 0x18);
                pdVar22 = (double *)(lVar11 + lVar17 * 8);
                pdVar22[1] = dVar37;
                *pdVar22 = dVar36;
                pdVar22[2] = dVar70;
                pdVar8 = pdVar22;
                pdVar24 = local_328;
                goto joined_r0x0001027b9cdc;
              }
LAB_1027bc1e0:
              func_0x00010002db08();
            }
            else {
              func_0x000100023e9c(&local_328);
            }
            goto LAB_1027bc2a8;
          }
          local_320[1] = dStack_f8;
          *local_320 = local_100;
          local_320[2] = local_f0;
          pdVar22 = local_320 + 3;
LAB_1027b9d38:
          pdVar18 = local_338;
          pdVar29 = local_340;
          dVar66 = dVar66 * dVar75;
          dVar72 = dVar72 * dVar75;
          dVar36 = (-(dVar65 * dVar76) + dVar74 * dVar69) * dVar75;
          dVar37 = dVar75 * (-(dVar64 * dVar69) + dVar65 * dVar68);
          dVar75 = dVar75 * (-(dVar77 * dVar65) + dVar78 * dVar64);
          local_320 = pdVar22;
          if (local_338 < local_330) {
            local_338[1] = dVar36;
            *local_338 = dVar66;
            local_338[3] = dStack_118;
            local_338[2] = local_120;
            local_338[5] = dStack_108;
            local_338[4] = local_110;
            local_338[7] = dVar37;
            local_338[6] = dVar72;
            local_338[8] = dVar75;
            local_338 = local_338 + 9;
            if (local_940 < local_938) goto LAB_1027b9da8;
LAB_1027b9f18:
            puVar12 = local_940;
            puVar31 = local_948;
            lVar17 = (long)local_940 - (long)local_948 >> 3;
            uVar34 = lVar17 * -0x5555555555555555 + 1;
            if (0xaaaaaaaaaaaaaaa < uVar34) {
              func_0x000100023e9c(&local_948);
              goto LAB_1027bc2a8;
            }
            lVar11 = (long)local_938 - (long)local_948 >> 3;
            uVar28 = lVar11 * 0x5555555555555556;
            if (uVar28 < uVar34 || uVar28 - uVar34 == 0) {
              uVar28 = uVar34;
            }
            if (0x555555555555554 < (ulong)(lVar11 * -0x5555555555555555)) {
              uVar28 = 0xaaaaaaaaaaaaaaa;
            }
            if (uVar28 == 0) {
              lVar11 = 0;
            }
            else {
              if (0xaaaaaaaaaaaaaaa < uVar28) {
                func_0x00010002db08();
                goto LAB_1027bc2a8;
              }
              lVar11 = func_0x000103462620(uVar28 * 0x18);
            }
            puVar23 = (undefined8 *)(lVar11 + lVar17 * 8);
            *puVar23 = uVar3;
            puVar23[1] = uVar6;
            puVar23[2] = uVar7;
            puVar27 = puVar23;
            puVar5 = puVar12;
            for (; puVar12 != puVar31; puVar12 = puVar12 + -3) {
              auVar39 = *(undefined1 (*) [16])(puVar12 + -3);
              puVar27[-1] = puVar12[-1];
              puVar27[-2] = auVar39._8_8_;
              puVar27[-3] = auVar39._0_8_;
              puVar27 = puVar27 + -3;
              puVar5 = local_948;
            }
            local_938 = (undefined8 *)(lVar11 + uVar28 * 0x18);
            puVar23 = puVar23 + 3;
            local_948 = puVar27;
            if (puVar5 != (undefined8 *)0x0) {
              local_940 = puVar23;
              func_0x000103462518(puVar5);
            }
            local_940 = puVar23;
            if (local_958 < local_950) goto LAB_1027b9a4c;
LAB_1027ba004:
            pdVar29 = local_958;
            pdVar21 = local_960;
            lVar17 = (long)local_958 - (long)local_960 >> 3;
            uVar34 = lVar17 * -0x5555555555555555 + 1;
            if (0xaaaaaaaaaaaaaaa < uVar34) {
              func_0x000100023e9c(&local_960);
              goto LAB_1027bc2a8;
            }
            lVar11 = (long)local_950 - (long)local_960 >> 3;
            uVar28 = lVar11 * 0x5555555555555556;
            if (uVar28 < uVar34 || uVar28 - uVar34 == 0) {
              uVar28 = uVar34;
            }
            if (0x555555555555554 < (ulong)(lVar11 * -0x5555555555555555)) {
              uVar28 = 0xaaaaaaaaaaaaaaa;
            }
            if (uVar28 == 0) {
              lVar11 = 0;
            }
            else {
              if (0xaaaaaaaaaaaaaaa < uVar28) {
                func_0x00010002db08();
                goto LAB_1027bc2a8;
              }
              lVar11 = func_0x000103462620(uVar28 * 0x18);
            }
            pdVar18 = (double *)(lVar11 + lVar17 * 8);
            pdVar18[1] = dVar71;
            *pdVar18 = dVar67;
            pdVar18[2] = dVar73;
            pdVar22 = pdVar18;
            pdVar24 = pdVar29;
            for (; pdVar29 != pdVar21; pdVar29 = pdVar29 + -3) {
              auVar39 = *(undefined1 (*) [16])(pdVar29 + -3);
              pdVar22[-1] = pdVar29[-1];
              pdVar22[-2] = auVar39._8_8_;
              pdVar22[-3] = auVar39._0_8_;
              pdVar22 = pdVar22 + -3;
              pdVar24 = local_960;
            }
            local_950 = (double *)(lVar11 + uVar28 * 0x18);
            pdVar18 = pdVar18 + 3;
            local_960 = pdVar22;
            if (pdVar24 != (double *)0x0) {
              func_0x000103462518(pdVar24);
            }
          }
          else {
            lVar17 = (long)local_338 - (long)local_340 >> 3;
            uVar34 = lVar17 * -0x71c71c71c71c71c7 + 1;
            if (0x38e38e38e38e38e < uVar34) {
              func_0x000100023e9c(&local_340);
              goto LAB_1027bc2a8;
            }
            lVar11 = (long)local_330 - (long)local_340 >> 3;
            uVar28 = lVar11 * 0x1c71c71c71c71c72;
            if (uVar28 < uVar34 || uVar28 - uVar34 == 0) {
              uVar28 = uVar34;
            }
            if (0x1c71c71c71c71c6 < (ulong)(lVar11 * -0x71c71c71c71c71c7)) {
              uVar28 = 0x38e38e38e38e38e;
            }
            if (uVar28 == 0) {
              lVar11 = 0;
            }
            else {
              if (0x38e38e38e38e38e < uVar28) goto LAB_1027bc1e0;
              lVar11 = func_0x000103462620(uVar28 * 0x48);
            }
            pdVar22 = (double *)(lVar11 + lVar17 * 8);
            pdVar22[1] = dVar36;
            *pdVar22 = dVar66;
            pdVar22[3] = dStack_118;
            pdVar22[2] = local_120;
            pdVar22[5] = dStack_108;
            pdVar22[4] = local_110;
            pdVar22[7] = dVar37;
            pdVar22[6] = dVar72;
            pdVar22[8] = dVar75;
            local_340 = pdVar22;
            pdVar21 = pdVar18;
            for (; pdVar18 != pdVar29; pdVar18 = pdVar18 + -9) {
              dVar36 = pdVar18[-9];
              local_340[-8] = pdVar18[-8];
              local_340[-9] = dVar36;
              auVar39 = *(undefined1 (*) [16])(pdVar18 + -7);
              auVar54 = *(undefined1 (*) [16])(pdVar18 + -5);
              dVar37 = pdVar18[-2];
              dVar36 = pdVar18[-3];
              local_340[-1] = pdVar18[-1];
              local_340[-2] = dVar37;
              local_340[-3] = dVar36;
              local_340[-4] = auVar54._8_8_;
              local_340[-5] = auVar54._0_8_;
              local_340[-6] = auVar39._8_8_;
              local_340[-7] = auVar39._0_8_;
              local_340 = local_340 + -9;
              pdVar21 = pdVar29;
            }
            local_330 = (double *)(lVar11 + uVar28 * 0x48);
            if (pdVar21 != (double *)0x0) {
              func_0x000103462518(pdVar21);
            }
            local_338 = pdVar22 + 9;
            if (local_938 <= local_940) goto LAB_1027b9f18;
LAB_1027b9da8:
            *local_940 = uVar3;
            local_940[1] = uVar6;
            puVar12 = local_940 + 3;
            local_940[2] = uVar7;
            local_940 = puVar12;
            if (local_950 <= local_958) goto LAB_1027ba004;
LAB_1027b9a4c:
            local_958[1] = dVar71;
            *local_958 = dVar67;
            local_958[2] = dVar73;
            pdVar18 = local_958 + 3;
          }
          uVar19 = uVar19 + 1;
          uVar34 = ((long)local_350 - (long)local_358 >> 3) * -0x5555555555555555;
          local_958 = pdVar18;
        } while (uVar19 <= uVar34 && uVar34 - uVar19 != 0);
      }
      bVar4 = false;
LAB_1027ba0f8:
      func_0x0001027d3e48(auStack_930);
      uVar33 = (int)((ulong)((long)local_350 - (long)local_358) >> 3) * -0x55555555;
      if (bVar4) {
        uVar19 = (ulong)(uVar33 - 1);
        if (-1 < (int)(uVar33 - 1)) {
LAB_1027ba264:
          pdVar18 = local_358 + uVar19 * 3;
          local_d0 = -*pdVar18;
          local_c8 = -pdVar18[1];
          local_c0 = -pdVar18[2];
          pdVar18 = local_370 + uVar19 * 3;
          local_e8 = *pdVar18;
          local_e0 = pdVar18[1];
          local_d8 = pdVar18[2];
          func_0x0001027d3cb0(auStack_930,&local_d0,&local_e8);
          func_0x0001027d3cdc(auStack_930,&local_140);
          func_0x0001027d3e40(auStack_930);
        }
LAB_1027ba658:
        local_320 = local_328;
        func_0x0001027cace0(auStack_238,0x32);
        if (local_350 != local_358) {
          uVar19 = 0;
          uVar34 = 0;
          do {
            pdVar18 = local_370 + uVar19 * 3;
            local_980 = *pdVar18 / 9.8;
            dStack_978 = pdVar18[1] / 9.8;
            local_970 = pdVar18[2] / 9.8;
            iVar10 = func_0x0001027cad68(auStack_238,local_358 + uVar19 * 3,&local_980);
            func_0x0001027cb4bc(&local_980,auStack_238);
            pdVar18 = local_320;
            pdVar29 = local_328;
            dVar73 = local_970;
            dVar71 = dStack_978;
            dVar67 = local_980;
            if (local_320 < local_318) {
              local_320[1] = dStack_978;
              *local_320 = local_980;
              local_320[2] = local_970;
              pdVar21 = local_320 + 3;
            }
            else {
              lVar17 = (long)local_320 - (long)local_328 >> 3;
              uVar28 = lVar17 * -0x5555555555555555 + 1;
              if (0xaaaaaaaaaaaaaaa < uVar28) {
                func_0x000100023e9c(&local_328);
                goto LAB_1027bc2a8;
              }
              lVar11 = (long)local_318 - (long)local_328 >> 3;
              uVar30 = lVar11 * 0x5555555555555556;
              if (uVar30 < uVar28 || uVar30 - uVar28 == 0) {
                uVar30 = uVar28;
              }
              if (0x555555555555554 < (ulong)(lVar11 * -0x5555555555555555)) {
                uVar30 = 0xaaaaaaaaaaaaaaa;
              }
              if (uVar30 == 0) {
                lVar11 = 0;
              }
              else {
                if (0xaaaaaaaaaaaaaaa < uVar30) {
                  func_0x00010002db08();
                  goto LAB_1027bc2a8;
                }
                lVar11 = func_0x000103462620(uVar30 * 0x18);
              }
              pdVar21 = (double *)(lVar11 + lVar17 * 8);
              pdVar21[1] = dVar71;
              *pdVar21 = dVar67;
              pdVar21[2] = dVar73;
              pdVar22 = pdVar21;
              pdVar24 = pdVar18;
              for (; pdVar18 != pdVar29; pdVar18 = pdVar18 + -3) {
                auVar39 = *(undefined1 (*) [16])(pdVar18 + -3);
                pdVar22[-1] = pdVar18[-1];
                pdVar22[-2] = auVar39._8_8_;
                pdVar22[-3] = auVar39._0_8_;
                pdVar22 = pdVar22 + -3;
                pdVar24 = local_328;
              }
              local_318 = (double *)(lVar11 + uVar30 * 0x18);
              pdVar21 = pdVar21 + 3;
              local_328 = pdVar22;
              if (pdVar24 != (double *)0x0) {
                local_320 = pdVar21;
                func_0x000103462518(pdVar24);
              }
            }
            uVar33 = (uint)uVar19;
            if ((uint)uVar34 != 0 || iVar10 != 1) {
              uVar33 = (uint)uVar34;
            }
            uVar34 = (ulong)uVar33;
            uVar19 = uVar19 + 1;
            uVar28 = ((long)local_350 - (long)local_358 >> 3) * -0x5555555555555555;
            local_320 = pdVar21;
          } while (uVar19 <= uVar28 && uVar28 - uVar19 != 0);
          if (0 < (int)uVar33) {
            lVar17 = 0;
            do {
              pdVar29 = local_328 + (ulong)uVar33 * 3;
              pdVar18 = (double *)((long)local_328 + lVar17);
              dVar67 = *pdVar29;
              pdVar18[1] = pdVar29[1];
              *pdVar18 = dVar67;
              pdVar18[2] = pdVar29[2];
              lVar17 = lVar17 + 0x18;
            } while (uVar34 * 0x18 - lVar17 != 0);
          }
        }
        func_0x0001027cad40(auStack_238);
      }
      else {
        while (uVar33 = uVar33 - 1, -1 < (int)uVar33) {
          pdVar18 = local_358 + (ulong)uVar33 * 3;
          local_d0 = -*pdVar18;
          local_c8 = -pdVar18[1];
          local_c0 = -pdVar18[2];
          pdVar18 = local_370 + (ulong)uVar33 * 3;
          local_e8 = *pdVar18;
          local_e0 = pdVar18[1];
          local_d8 = pdVar18[2];
          func_0x0001027d3cb0(auStack_930,&local_d0,&local_e8);
        }
        func_0x0001027d20e4(auStack_930);
        if (local_350 != local_358) {
          lVar17 = 0;
          uVar19 = 0;
          do {
            pdVar18 = (double *)((long)local_358 + lVar17);
            local_d0 = *pdVar18;
            local_c8 = pdVar18[1];
            local_c0 = pdVar18[2];
            pdVar18 = (double *)((long)local_370 + lVar17);
            local_e8 = *pdVar18;
            local_e0 = pdVar18[1];
            local_d8 = pdVar18[2];
            func_0x0001027d3cb0(auStack_930,&local_d0,&local_e8);
            uVar19 = uVar19 + 1;
            uVar34 = ((long)local_350 - (long)local_358 >> 3) * -0x5555555555555555;
            lVar17 = lVar17 + 0x18;
          } while (uVar19 <= uVar34 && uVar34 - uVar19 != 0);
        }
        func_0x0001027d20e4(auStack_930);
        iVar10 = (int)((ulong)((long)local_350 - (long)local_358) >> 3) * -0x55555555;
        uVar19 = (ulong)(iVar10 - 1U);
        if (-1 < (int)(iVar10 - 1U)) {
          if (bVar4) goto LAB_1027ba264;
          lVar11 = uVar19 * 0x18;
          lVar17 = uVar19 * 0x48 + 0x20;
          do {
            pdVar18 = (double *)((long)local_358 + lVar11);
            local_d0 = -*pdVar18;
            local_c8 = -pdVar18[1];
            local_c0 = -pdVar18[2];
            pdVar18 = (double *)((long)local_370 + lVar11);
            local_e8 = *pdVar18;
            local_e0 = pdVar18[1];
            local_d8 = pdVar18[2];
            func_0x0001027d3cb0(auStack_930,&local_d0,&local_e8);
            func_0x0001027d3cdc(auStack_930,&local_140);
            lVar25 = func_0x0001027d3e40(auStack_930);
            if (10.6 <= SQRT(local_4a0 * local_4a0 + local_4b0 * local_4b0 + dStack_4a8 * dStack_4a8
                            )) goto LAB_1027ba658;
            dVar66 = *(double *)(lVar25 + 0xd0);
            dVar67 = *(double *)(lVar25 + 0xd8);
            dVar71 = *(double *)(lVar25 + 0xe0);
            dVar73 = *(double *)(lVar25 + 0xe8);
            dVar72 = *(double *)(lVar25 + 0xf0);
            dVar36 = *(double *)(lVar25 + 0xf8);
            dVar53 = *(double *)(lVar25 + 0x100);
            dVar65 = *(double *)(lVar25 + 0x108);
            dVar37 = *(double *)(lVar25 + 0x110);
            dVar76 = -(dVar36 * dVar65) + dVar37 * dVar72;
            dVar77 = -(dVar37 * dVar67) + dVar71 * dVar65;
            dVar78 = -(dVar71 * dVar72) + dVar36 * dVar67;
            dVar70 = 1.0 / (dVar78 * dVar53 + dVar66 * dVar76 + dVar73 * dVar77);
            dVar76 = dVar76 * dVar70;
            dVar64 = (-(dVar73 * dVar37) + dVar53 * dVar36) * dVar70;
            dVar77 = dVar77 * dVar70;
            dVar78 = dVar78 * dVar70;
            dVar69 = dVar70 * (-(dVar53 * dVar71) + dVar66 * dVar37);
            dVar37 = dVar70 * (-(dVar66 * dVar36) + dVar73 * dVar71);
            dVar36 = (-(dVar72 * dVar53) + dVar65 * dVar73) * dVar70;
            dVar71 = dVar70 * (-(dVar65 * dVar66) + dVar67 * dVar53);
            dVar70 = dVar70 * (-(dVar67 * dVar73) + dVar72 * dVar66);
            pdVar18 = (double *)((long)local_340 + lVar17);
            pdVar29 = (double *)((long)local_328 + lVar11);
            dVar67 = pdVar18[-4];
            dVar73 = *pdVar29;
            dVar53 = pdVar29[1];
            dVar72 = pdVar18[-1];
            dVar66 = pdVar18[2];
            dVar74 = pdVar29[2];
            dVar65 = pdVar18[-2];
            dVar68 = pdVar18[1];
            dVar75 = pdVar18[4];
            pdVar29[1] = pdVar18[-3] * dVar73 + *pdVar18 * dVar53 + pdVar18[3] * dVar74;
            *pdVar29 = dVar67 * dVar73 + dVar72 * dVar53 + dVar66 * dVar74;
            pdVar29[2] = dVar73 * dVar65 + dVar53 * dVar68 + dVar74 * dVar75;
            pdVar18 = (double *)((long)local_328 + lVar11);
            pdVar18[1] = pdVar18[1] -
                         (dVar64 * local_140 + dVar69 * dStack_138 + dVar37 * local_130);
            *pdVar18 = *pdVar18 - (dVar76 * local_140 + dVar77 * dStack_138 + dVar78 * local_130);
            pdVar18[2] = pdVar18[2] -
                         (dVar36 * local_140 + dVar71 * dStack_138 + local_130 * dVar70);
            pdVar18 = (double *)((long)local_340 + lVar17);
            pdVar18[-3] = dVar64 + pdVar18[-3];
            pdVar18[-4] = dVar76 + pdVar18[-4];
            pdVar18[-1] = dVar77 + pdVar18[-1];
            pdVar18[-2] = dVar36 + pdVar18[-2];
            pdVar18[1] = dVar71 + pdVar18[1];
            *pdVar18 = dVar69 + *pdVar18;
            pdVar18[3] = dVar37 + pdVar18[3];
            pdVar18[2] = dVar78 + pdVar18[2];
            pdVar18[4] = dVar70 + pdVar18[4];
            pdVar18 = (double *)((long)local_340 + lVar17);
            dVar64 = *pdVar18;
            dVar66 = pdVar18[3];
            dVar37 = pdVar18[4];
            dVar53 = *(double *)*(undefined1 (*) [16])(pdVar18 + 1);
            auVar39 = *(undefined1 (*) [16])(pdVar18 + -4);
            dVar67 = pdVar18[-2];
            dVar71 = pdVar18[-1];
            dVar36 = auVar39._8_8_;
            dVar73 = auVar39._0_8_;
            dStack_118 = dVar71 * -dVar37 + pdVar18[2] * dVar53;
            auVar39 = NEON_ext(*(undefined1 (*) [16])(pdVar18 + 1),auVar39,8,1);
            dVar75 = dVar64 * -auVar39._0_8_ + dVar66 * dVar71;
            dVar70 = dVar53 * -dVar66 + dVar37 * dVar64;
            dVar72 = 1.0 / (dVar75 * dVar67 + dVar73 * dVar70 + dVar36 * dStack_118);
            local_120 = (dVar67 * -dVar64 + dVar53 * dVar36) * dVar72;
            dStack_118 = dStack_118 * dVar72;
            local_110 = (auVar39._0_8_ * -dVar67 + dVar73 * dVar37) * dVar72;
            dStack_108 = (auVar39._8_8_ * -dVar53 + dVar71 * dVar67) * dVar72;
            pdVar29 = (double *)((long)local_328 + lVar11);
            dVar53 = *pdVar29;
            dVar65 = pdVar29[1];
            dVar68 = pdVar29[2];
            pdVar29[1] = (dVar37 * -dVar36 + dVar67 * dVar66) * dVar72 * dVar53 + local_110 * dVar65
                         + (dVar66 * -auVar39._8_8_ + dVar36 * pdVar18[2]) * dVar72 * dVar68;
            *pdVar29 = dVar70 * dVar72 * dVar53 + dStack_118 * dVar65 + dVar75 * dVar72 * dVar68;
            pdVar29[2] = dVar53 * local_120 +
                         dVar65 * dStack_108 +
                         dVar72 * (-dVar71 * dVar36 + dVar64 * dVar73) * dVar68;
            iVar10 = iVar10 + -1;
            lVar11 = lVar11 + -0x18;
            lVar17 = lVar17 + -0x48;
          } while (0 < iVar10);
        }
        if (bVar4) goto LAB_1027ba658;
      }
      local_428[0] = 0x4034000000000000;
      local_970 = 0.0;
      dStack_968 = 1.0;
      pdVar18 = (double *)*param_2;
      dVar71 = pdVar18[0x13];
      dVar73 = pdVar18[9];
      local_980 = 0.0;
      dStack_978 = 0.0;
      local_990 = (double *)0x0;
      pdStack_988 = (double *)0x0;
      local_998 = (double *)0x0;
      dVar67 = dVar73;
      if (param_2[1] - (long)pdVar18 != 0) {
        lVar17 = param_2[1] - (long)pdVar18 >> 4;
        if (0xaaaaaaaaaaaaaaa < (ulong)(lVar17 * -0x3333333333333333)) {
          func_0x000100023e9c(&local_998);
          goto LAB_1027bc2a8;
        }
        local_998 = (double *)func_0x000103462620(lVar17 * 0x3333333333333338);
        pdStack_988 = local_998 + lVar17 * 0x666666666666667;
        local_990 = local_998 + lVar17 * 0x666666666666667;
        dVar67 = pdVar18[9];
      }
      pdVar21 = local_990;
      pdVar29 = local_998;
      local_310 = *pdVar18;
      dStack_308 = pdVar18[1];
      local_300 = pdVar18[2];
      dStack_2f0 = pdVar18[4];
      local_2f8 = pdVar18[3];
      local_2e8 = pdVar18[5];
      local_2e0 = pdVar18[6];
      dStack_2d8 = pdVar18[7];
      local_2d0 = pdVar18[8];
      local_2c8 = dVar67;
      func_0x0001027b0f4c(&local_310,*(undefined4 *)(param_1 + 0x58));
      uVar33 = uVar32 >> 1;
      uVar19 = (ulong)uVar33;
      lVar17 = param_2[1];
      auVar54 = NEON_fmov(0x3fe0000000000000,8);
      auVar39 = NEON_fmov(0x3ff0000000000000,8);
      dVar67 = auVar39._8_8_;
      dVar37 = auVar54._8_8_;
      dVar36 = auVar54._0_8_;
      dVar70 = 1.0 / dVar35;
      if (lVar17 != *param_2) {
        uVar34 = 0;
        dVar71 = dVar71 - dVar73;
        lVar11 = 0x48;
        local_9e0 = 0.0;
        local_9f0 = 0.0;
        local_a00 = 1.0;
        lVar17 = *param_2;
        pdVar18 = pdVar29 + 2;
        dVar73 = 0.0;
        do {
          if (uVar19 < uVar34) {
            if ((long)uVar34 < (long)(int)(uVar32 * iVar15 - uVar33)) {
              uVar1 = 0;
              if (uVar32 != 0) {
                uVar1 = (int)((int)uVar34 - uVar33) / (int)uVar32;
              }
              iVar10 = 0;
              if (uVar32 != 0) {
                iVar10 = (int)(uVar1 - uVar33) / (int)uVar32;
              }
              dVar72 = dVar70 * (double)(int)((uVar1 - uVar33) - iVar10 * uVar32);
              pdVar24 = local_328 + (ulong)uVar1 * 3;
              dVar66 = 1.0 - dVar72;
              pdVar22 = local_328 + (ulong)(uVar1 + 1) * 3;
              dVar53 = *pdVar22 * dVar72 + *pdVar24 * dVar66;
              dStack_a48 = pdVar22[1] * dVar72 + pdVar24[1] * dVar66;
              dVar72 = dVar66 * pdVar24[2] + dVar72 * pdVar22[2];
            }
            else {
              dVar53 = local_320[-3];
              dStack_a48 = local_320[-2];
              dVar72 = local_320[-1];
            }
          }
          else {
            dVar53 = *local_328;
            dStack_a48 = local_328[1];
            dVar72 = local_328[2];
          }
          pdVar22 = (double *)(lVar17 + lVar11);
          local_2c0 = pdVar22[-9];
          dStack_2b8 = pdVar22[-8];
          local_2b0 = pdVar22[-7];
          local_2a8 = pdVar22[-6];
          dStack_2a0 = pdVar22[-5];
          local_298 = pdVar22[-4];
          local_290 = pdVar22[-3];
          dStack_288 = pdVar22[-2];
          local_280 = pdVar22[-1];
          local_278 = *pdVar22;
          func_0x0001027b0f4c(&local_2c0,*(undefined4 *)(param_1 + 0x58));
          dVar66 = local_2a8;
          if (52.0 < ABS(local_2a8 - local_2f8)) {
            dVar66 = local_2f8;
          }
          local_2f8 = dVar66;
          if (ABS(dStack_2a0 - dStack_2f0) <= 52.0) {
            dStack_2f0 = dStack_2a0;
          }
          dVar66 = local_298;
          if (52.0 < ABS(local_298 - local_2e8)) {
            dVar66 = local_2e8;
          }
          local_2e8 = dVar66;
          dVar66 = local_310;
          if (ABS(local_2c0 - local_310) <= 45.0) {
            dVar66 = local_2c0;
          }
          dVar64 = dStack_2b8;
          if (45.0 < ABS(dStack_2b8 - dStack_308)) {
            dVar64 = dStack_308;
          }
          if (45.0 < ABS(local_2b0 - local_300)) {
            local_2b0 = local_300;
          }
          dVar65 = local_2b0;
          dStack_2d8 = dStack_288;
          local_2e0 = local_290;
          local_2c8 = local_278;
          local_2d0 = local_280;
          lVar17 = *param_2;
          if (uVar34 == 0) {
            dVar68 = *(double *)(lVar17 + 0x48);
          }
          else {
            dVar68 = *(double *)(lVar17 + lVar11);
            dVar71 = dVar68 - *(double *)(lVar17 + (ulong)((int)uVar34 - 1) * 0x50 + 0x48);
          }
          dVar53 = (-dStack_2f0 - dVar53) * dVar71;
          dVar75 = ((local_2f8 + 0.0) - dStack_a48) * dVar71;
          dVar72 = ((local_2e8 + 0.0) - dVar72) * dVar71;
          auVar39._0_8_ = SQRT(dVar72 * dVar72 + dVar53 * dVar53 + dVar75 * dVar75);
          auVar39._8_8_ = 0;
          local_310 = dVar66;
          dStack_308 = dVar64;
          local_300 = local_2b0;
          local_2c0 = dVar66;
          dStack_2b8 = dVar64;
          local_2a8 = local_2f8;
          dStack_2a0 = dStack_2f0;
          local_298 = local_2e8;
          if (auVar39._0_8_ <= 1e-05) {
            dVar53 = dVar53 * dVar36;
            dVar75 = dVar75 * dVar37;
            dVar72 = dVar72 * 0.5;
            dVar69 = dVar72 * dVar72 + dVar53 * dVar53 + dVar67 + dVar75 * dVar75;
            dVar74 = dVar67;
          }
          else {
            auVar40._0_8_ = auVar39._0_8_ * 0.5;
            auVar40._8_8_ = 0;
            auVar54 = auVar39;
            dVar69 = (double)func_0x0001034627b8(auVar40);
            dVar72 = (dVar72 * dVar69) / auVar39._0_8_;
            dVar74 = auVar54._0_8_;
            dVar53 = (dVar53 * dVar69) / auVar39._0_8_;
            dVar75 = (dVar75 * dVar69) / auVar39._0_8_;
            dVar69 = dVar53 * dVar53 + dVar72 * dVar72 + dVar75 * dVar75 + dVar74 * dVar74;
          }
          if (0.0 < dVar69) {
            dVar69 = SQRT(dVar69);
            dVar53 = dVar53 / dVar69;
            dVar75 = dVar75 / dVar69;
            dVar72 = dVar72 / dVar69;
            dVar74 = dVar74 / dVar69;
          }
          dStack_968 = ((-local_9f0 * dVar53 + local_a00 * dVar74) - dVar73 * dVar75) -
                       local_9e0 * dVar72;
          local_980 = (local_9f0 * dVar74 + local_a00 * dVar53 + dVar73 * dVar72) -
                      local_9e0 * dVar75;
          dVar69 = (dVar73 * dVar74 + local_a00 * dVar75 + local_9e0 * dVar53) - local_9f0 * dVar72;
          local_970 = (local_9e0 * dVar74 + local_a00 * dVar72 + local_9f0 * dVar75) -
                      dVar73 * dVar53;
          dVar73 = local_980 * local_980 + local_970 * local_970 +
                   dVar69 * dVar69 + dStack_968 * dStack_968;
          if (0.0 < dVar73) {
            dVar73 = SQRT(dVar73);
            local_980 = local_980 / dVar73;
            dVar69 = dVar69 / dVar73;
            local_970 = local_970 / dVar73;
            dStack_968 = dStack_968 / dVar73;
          }
          local_a00 = dStack_968;
          local_9e0 = local_970;
          local_9f0 = local_980;
          auVar54._0_8_ = dVar68 + 0.001;
          auVar54._8_8_ = 0;
          dStack_978 = dVar69;
          func_0x0001027b7bb4(auVar54,param_1,&local_980);
          dVar53 = dVar66 * local_9e0 + -dVar65 * dVar69;
          dVar72 = dVar65 * local_9f0 + dVar64 * local_9e0;
          dVar73 = -(dVar69 * dVar64) + -dVar66 * local_9f0;
          dVar53 = dVar53 + dVar53;
          dVar72 = dVar72 + dVar72;
          dVar73 = dVar73 + dVar73;
          pdVar18[-1] = -dVar66 + dVar72 * local_a00 + -(local_9f0 * dVar73) + local_9e0 * dVar53;
          pdVar18[-2] = dVar64 + dVar53 * local_a00 + -local_9e0 * dVar72 + dVar73 * dVar69;
          *pdVar18 = (local_a00 * dVar73 - dVar65) + -dVar69 * dVar53 + local_9f0 * dVar72;
          uVar34 = uVar34 + 1;
          lVar17 = *param_2;
          uVar28 = (param_2[1] - lVar17 >> 4) * -0x3333333333333333;
          lVar11 = lVar11 + 0x50;
          pdVar18 = pdVar18 + 3;
          dVar73 = dVar69;
        } while (uVar34 <= uVar28 && uVar28 - uVar34 != 0);
      }
      local_9b0 = (double *)0x0;
      local_9a8 = (double *)0x0;
      local_9a0 = (double *)0x0;
      local_9c8 = (double *)0x0;
      local_9c0 = (double *)0x0;
      local_9b8 = (double *)0x0;
      iVar16 = (int)((ulong)((long)pdVar21 - (long)pdVar29) >> 3) * -0x55555555;
      iVar10 = iVar16 + -1;
      if (29999 < iVar16) {
        iVar10 = 30000;
      }
      if (iVar10 != 0) {
        if (iVar10 < 0) {
          func_0x000100023e9c(&local_9b0);
          goto LAB_1027bc2a8;
        }
        local_9b0 = (double *)func_0x000103462620((long)iVar10 * 0x18);
        local_9a0 = local_9b0 + (long)iVar10 * 3;
        local_9a8 = local_9b0;
        local_9c0 = (double *)func_0x000103462620((long)iVar10 * 0x18);
        local_9b8 = local_9c0 + (long)iVar10 * 3;
      }
      local_310 = *(double *)(lVar17 + 0x50);
      dStack_308 = *(double *)(lVar17 + 0x58);
      local_300 = *(double *)(lVar17 + 0x60);
      local_2f8 = *(double *)(lVar17 + 0x68);
      dStack_2f0 = *(double *)(lVar17 + 0x70);
      local_2e8 = *(double *)(lVar17 + 0x78);
      dStack_2d8 = SUB168(*(undefined1 (*) [16])(lVar17 + 0x80),8);
      local_2e0 = SUB168(*(undefined1 (*) [16])(lVar17 + 0x80),0);
      local_2c8 = SUB168(*(undefined1 (*) [16])(lVar17 + 0x90),8);
      local_2d0 = SUB168(*(undefined1 (*) [16])(lVar17 + 0x90),0);
      local_9c8 = local_9c0;
      func_0x0001027b0f4c(&local_310,*(undefined4 *)(param_1 + 0x58));
      uVar34 = *(ulong *)(param_1 + 0xa0);
      uVar28 = 0;
      if (uVar34 != 0) {
        uVar28 = *(ulong *)(param_1 + 0xb0) / uVar34;
      }
      pdVar18 = (double *)
                (*(long *)(param_1 + 0x98) + (*(ulong *)(param_1 + 0xb0) - uVar28 * uVar34) * 0x20);
      local_970 = pdVar18[2];
      local_980 = *pdVar18;
      dStack_978 = pdVar18[1];
      dStack_968 = pdVar18[3];
      if (iVar10 != 0) {
        uVar34 = 1;
        do {
          while( true ) {
            if (uVar19 < uVar34) {
              if ((long)uVar34 < (long)(int)(uVar32 * iVar15 - uVar33)) {
                uVar1 = 0;
                if (uVar32 != 0) {
                  uVar1 = (int)((int)uVar34 - uVar33) / (int)uVar32;
                }
                iVar16 = 0;
                if (uVar32 != 0) {
                  iVar16 = (int)(uVar1 - uVar33) / (int)uVar32;
                }
                dVar71 = dVar70 * (double)(int)((uVar1 - uVar33) - iVar16 * uVar32);
                pdVar29 = local_328 + (ulong)uVar1 * 3;
                dVar73 = 1.0 - dVar71;
                pdVar18 = local_328 + (ulong)(uVar1 + 1) * 3;
                local_9e0 = *pdVar18 * dVar71 + *pdVar29 * dVar73;
                dStack_9d8 = pdVar18[1] * dVar71 + pdVar29[1] * dVar73;
                dVar71 = dVar73 * pdVar29[2] + dVar71 * pdVar18[2];
              }
              else {
                local_9e0 = local_320[-3];
                dStack_9d8 = local_320[-2];
                dVar71 = local_320[-1];
              }
            }
            else {
              local_9e0 = *local_328;
              dStack_9d8 = local_328[1];
              dVar71 = local_328[2];
            }
            pdVar18 = (double *)(*param_2 + uVar34 * 0x50);
            local_2c0 = *pdVar18;
            dStack_2b8 = pdVar18[1];
            local_2b0 = pdVar18[2];
            local_2a8 = pdVar18[3];
            dStack_2a0 = pdVar18[4];
            local_298 = pdVar18[5];
            local_290 = pdVar18[6];
            dStack_288 = pdVar18[7];
            local_280 = pdVar18[8];
            local_278 = pdVar18[9];
            func_0x0001027b0f4c(&local_2c0,*(undefined4 *)(param_1 + 0x58));
            dVar73 = local_2a8;
            if (52.0 < ABS(local_2a8 - local_2f8)) {
              dVar73 = local_2f8;
            }
            local_2f8 = dVar73;
            if (ABS(dStack_2a0 - dStack_2f0) <= 52.0) {
              dStack_2f0 = dStack_2a0;
            }
            dVar73 = local_298;
            if (52.0 < ABS(local_298 - local_2e8)) {
              dVar73 = local_2e8;
            }
            local_2e8 = dVar73;
            dVar73 = local_310;
            if (ABS(local_2c0 - local_310) <= 45.0) {
              dVar73 = local_2c0;
            }
            dVar53 = dStack_2b8;
            if (45.0 < ABS(dStack_2b8 - dStack_308)) {
              dVar53 = dStack_308;
            }
            if (45.0 < ABS(local_2b0 - local_300)) {
              local_2b0 = local_300;
            }
            dVar72 = local_2b0;
            dStack_2d8 = dStack_288;
            local_2e0 = local_290;
            local_2c8 = local_278;
            local_2d0 = local_280;
            lVar17 = *param_2 + uVar34 * 0x50;
            dVar66 = *(double *)(lVar17 + 0x48) - *(double *)(lVar17 + -8);
            dVar64 = (-dStack_2f0 - local_9e0) * dVar66;
            dVar65 = ((local_2f8 + 0.0) - dStack_9d8) * dVar66;
            dVar66 = ((local_2e8 + 0.0) - dVar71) * dVar66;
            auVar55._0_8_ = SQRT(dVar66 * dVar66 + dVar64 * dVar64 + dVar65 * dVar65);
            auVar55._8_8_ = 0;
            local_310 = dVar73;
            dStack_308 = dVar53;
            local_300 = local_2b0;
            local_2c0 = dVar73;
            dStack_2b8 = dVar53;
            local_2a8 = local_2f8;
            dStack_2a0 = dStack_2f0;
            local_298 = local_2e8;
            if (auVar55._0_8_ <= 1e-05) {
              dVar64 = dVar64 * dVar36;
              dVar65 = dVar65 * dVar37;
              dVar66 = dVar66 * 0.5;
              dVar68 = dVar66 * dVar66 + dVar64 * dVar64 + dVar67 + dVar65 * dVar65;
              dVar71 = dVar67;
              if (0.0 < dVar68) {
                dVar68 = SQRT(dVar68);
                dVar64 = dVar64 / dVar68;
                dVar65 = dVar65 / dVar68;
                dVar66 = dVar66 / dVar68;
                dVar71 = dVar67 / dVar68;
              }
            }
            else {
              auVar41._0_8_ = auVar55._0_8_ * 0.5;
              auVar41._8_8_ = 0;
              auVar39 = auVar55;
              dVar68 = (double)func_0x0001034627b8(auVar41);
              dVar66 = (dVar66 * dVar68) / auVar55._0_8_;
              dVar71 = auVar39._0_8_;
              dVar64 = (dVar64 * dVar68) / auVar55._0_8_;
              dVar65 = (dVar65 * dVar68) / auVar55._0_8_;
              dVar68 = dVar64 * dVar64 + dVar66 * dVar66 + dVar65 * dVar65 + dVar71 * dVar71;
              if (0.0 < dVar68) {
                dVar68 = SQRT(dVar68);
                dVar64 = dVar64 / dVar68;
                dVar65 = dVar65 / dVar68;
                dVar66 = dVar66 / dVar68;
                dVar71 = dVar71 / dVar68;
              }
            }
            pdVar18 = local_9a8;
            pdVar29 = local_9b0;
            dVar68 = ((-local_980 * dVar64 + dStack_968 * dVar71) - dStack_978 * dVar65) -
                     local_970 * dVar66;
            dVar75 = (local_980 * dVar71 + dStack_968 * dVar64 + dStack_978 * dVar66) -
                     local_970 * dVar65;
            dVar69 = (dStack_978 * dVar71 + dStack_968 * dVar65 + local_970 * dVar64) -
                     local_980 * dVar66;
            local_970 = (local_970 * dVar71 + dStack_968 * dVar66 + local_980 * dVar65) -
                        dStack_978 * dVar64;
            dVar71 = dVar75 * dVar75 + local_970 * local_970 + dVar69 * dVar69 + dVar68 * dVar68;
            if (0.0 < dVar71) {
              dVar71 = SQRT(dVar71);
              dVar75 = dVar75 / dVar71;
              dVar69 = dVar69 / dVar71;
              local_970 = local_970 / dVar71;
              dVar68 = dVar68 / dVar71;
            }
            dVar64 = dVar73 * local_970 + -dVar72 * dVar69;
            dVar65 = dVar72 * dVar75 + dVar53 * local_970;
            dVar66 = -(dVar69 * dVar53) + -dVar73 * dVar75;
            dVar64 = dVar64 + dVar64;
            dVar65 = dVar65 + dVar65;
            dVar66 = dVar66 + dVar66;
            dVar71 = dVar53 + dVar64 * dVar68 + -local_970 * dVar65 + dVar66 * dVar69;
            dVar73 = -dVar73 + dVar65 * dVar68 + -(dVar75 * dVar66) + local_970 * dVar64;
            dVar53 = (dVar68 * dVar66 - dVar72) + -dVar69 * dVar64 + dVar75 * dVar65;
            local_980 = dVar75;
            dStack_978 = dVar69;
            dStack_968 = dVar68;
            if (local_9a0 <= local_9a8) break;
            local_9a8[1] = dVar73;
            *local_9a8 = dVar71;
            local_9a8[2] = dVar53;
            local_9a8 = local_9a8 + 3;
            uVar34 = uVar34 + 1;
            if (uVar34 == iVar10 + 1) goto LAB_1027bb574;
          }
          lVar17 = (long)local_9a8 - (long)local_9b0 >> 3;
          uVar28 = lVar17 * -0x5555555555555555 + 1;
          if (0xaaaaaaaaaaaaaaa < uVar28) {
            func_0x000100023e9c(&local_9b0);
            goto LAB_1027bc2a8;
          }
          lVar11 = (long)local_9a0 - (long)local_9b0 >> 3;
          uVar30 = lVar11 * 0x5555555555555556;
          if (uVar30 < uVar28 || uVar30 - uVar28 == 0) {
            uVar30 = uVar28;
          }
          if (0x555555555555554 < (ulong)(lVar11 * -0x5555555555555555)) {
            uVar30 = 0xaaaaaaaaaaaaaaa;
          }
          if (uVar30 == 0) {
            lVar11 = 0;
          }
          else {
            if (0xaaaaaaaaaaaaaaa < uVar30) {
              func_0x00010002db08();
              goto LAB_1027bc2a8;
            }
            lVar11 = func_0x000103462620(uVar30 * 0x18);
          }
          pdVar21 = (double *)(lVar11 + lVar17 * 8);
          pdVar21[1] = dVar73;
          *pdVar21 = dVar71;
          pdVar21[2] = dVar53;
          pdVar22 = pdVar21;
          pdVar24 = pdVar18;
          for (; pdVar18 != pdVar29; pdVar18 = pdVar18 + -3) {
            auVar39 = *(undefined1 (*) [16])(pdVar18 + -3);
            pdVar22[-1] = pdVar18[-1];
            pdVar22[-2] = auVar39._8_8_;
            pdVar22[-3] = auVar39._0_8_;
            pdVar22 = pdVar22 + -3;
            pdVar24 = local_9b0;
          }
          local_9a0 = (double *)(lVar11 + uVar30 * 0x18);
          pdVar21 = pdVar21 + 3;
          local_9b0 = pdVar22;
          if (pdVar24 != (double *)0x0) {
            local_9a8 = pdVar21;
            func_0x000103462518(pdVar24);
          }
          uVar34 = uVar34 + 1;
          local_9a8 = pdVar21;
        } while (uVar34 != iVar10 + 1);
      }
LAB_1027bb574:
      lVar17 = *param_2;
      lVar11 = param_2[1];
      local_310 = *(double *)(lVar11 + -0x50);
      dStack_308 = *(double *)(lVar11 + -0x48);
      local_300 = *(double *)(lVar11 + -0x40);
      local_2f8 = *(double *)(lVar11 + -0x38);
      dStack_2f0 = *(double *)(lVar11 + -0x30);
      local_2e8 = *(double *)(lVar11 + -0x28);
      local_2e0 = *(double *)(lVar11 + -0x20);
      dStack_2d8 = *(double *)(lVar11 + -0x18);
      local_2d0 = *(double *)(lVar11 + -0x10);
      local_2c8 = *(double *)(lVar11 + -8);
      func_0x0001027b0f4c(&local_310,*(undefined4 *)(param_1 + 0x58));
      if (*(long *)(param_1 + 0xa8) == 0) goto LAB_1027bc234;
      uVar30 = (*(long *)(param_1 + 0xa8) + *(long *)(param_1 + 0xb0)) - 1;
      uVar34 = *(ulong *)(param_1 + 0xa0);
      uVar28 = 0;
      if (uVar34 != 0) {
        uVar28 = uVar30 / uVar34;
      }
      pdVar18 = (double *)(*(long *)(param_1 + 0x98) + (uVar30 - uVar28 * uVar34) * 0x20);
      local_970 = pdVar18[2];
      local_980 = *pdVar18;
      dStack_978 = pdVar18[1];
      dStack_968 = pdVar18[3];
      if (0 < iVar10) {
        iVar16 = (int)(lVar11 - lVar17 >> 4) * -0x33333333;
        lVar11 = (long)(iVar16 - iVar10);
        lVar17 = (long)iVar16;
        do {
          while( true ) {
            lVar25 = lVar17 + -1;
            if ((long)uVar19 < lVar25) {
              if ((int)(uVar32 * iVar15 - uVar33) < lVar17) {
                local_9e0 = local_320[-3];
                dStack_9d8 = local_320[-2];
                dVar71 = local_320[-1];
              }
              else {
                uVar1 = 0;
                if (uVar32 != 0) {
                  uVar1 = (int)((int)lVar25 - uVar33) / (int)uVar32;
                }
                iVar10 = 0;
                if (uVar32 != 0) {
                  iVar10 = (int)(uVar1 - uVar33) / (int)uVar32;
                }
                dVar71 = dVar70 * (double)(int)((uVar1 - uVar33) - iVar10 * uVar32);
                pdVar29 = local_328 + (ulong)uVar1 * 3;
                dVar73 = 1.0 - dVar71;
                pdVar18 = local_328 + (ulong)(uVar1 + 1) * 3;
                local_9e0 = *pdVar18 * dVar71 + *pdVar29 * dVar73;
                dStack_9d8 = pdVar18[1] * dVar71 + pdVar29[1] * dVar73;
                dVar71 = dVar73 * pdVar29[2] + dVar71 * pdVar18[2];
              }
            }
            else {
              local_9e0 = *local_328;
              dStack_9d8 = local_328[1];
              dVar71 = local_328[2];
            }
            pdVar18 = (double *)(*param_2 + lVar25 * 0x50);
            local_2c0 = *pdVar18;
            dStack_2b8 = pdVar18[1];
            local_2b0 = pdVar18[2];
            local_2a8 = pdVar18[3];
            dStack_2a0 = pdVar18[4];
            local_298 = pdVar18[5];
            local_290 = pdVar18[6];
            dStack_288 = pdVar18[7];
            local_280 = pdVar18[8];
            local_278 = pdVar18[9];
            func_0x0001027b0f4c(&local_2c0,*(undefined4 *)(param_1 + 0x58));
            dVar73 = local_2a8;
            if (52.0 < ABS(local_2a8 - local_2f8)) {
              dVar73 = local_2f8;
            }
            local_2f8 = dVar73;
            if (ABS(dStack_2a0 - dStack_2f0) <= 52.0) {
              dStack_2f0 = dStack_2a0;
            }
            dVar73 = local_298;
            if (52.0 < ABS(local_298 - local_2e8)) {
              dVar73 = local_2e8;
            }
            local_2e8 = dVar73;
            dVar73 = local_310;
            if (ABS(local_2c0 - local_310) <= 45.0) {
              dVar73 = local_2c0;
            }
            dVar53 = dStack_2b8;
            if (45.0 < ABS(dStack_2b8 - dStack_308)) {
              dVar53 = dStack_308;
            }
            if (45.0 < ABS(local_2b0 - local_300)) {
              local_2b0 = local_300;
            }
            dVar72 = local_2b0;
            dStack_2d8 = dStack_288;
            local_2e0 = local_290;
            local_2c8 = local_278;
            local_2d0 = local_280;
            dVar66 = *(double *)(*param_2 + lVar25 * 0x50 + 0x48) -
                     *(double *)(*param_2 + lVar17 * 0x50 + -0x58);
            dVar64 = (local_9e0 + dStack_2f0) * dVar66;
            dVar65 = (dStack_9d8 + -(local_2f8 + 0.0)) * dVar66;
            dVar66 = (dVar71 - (local_2e8 + 0.0)) * dVar66;
            auVar56._0_8_ = SQRT(dVar66 * dVar66 + dVar64 * dVar64 + dVar65 * dVar65);
            auVar56._8_8_ = 0;
            local_310 = dVar73;
            dStack_308 = dVar53;
            local_300 = local_2b0;
            local_2c0 = dVar73;
            dStack_2b8 = dVar53;
            local_2a8 = local_2f8;
            dStack_2a0 = dStack_2f0;
            local_298 = local_2e8;
            if (auVar56._0_8_ <= 1e-05) {
              dVar64 = dVar64 * dVar36;
              dVar65 = dVar65 * dVar37;
              dVar66 = dVar66 * 0.5;
              dVar68 = dVar66 * dVar66 + dVar64 * dVar64 + dVar67 + dVar65 * dVar65;
              dVar71 = dVar67;
              if (0.0 < dVar68) {
                dVar68 = SQRT(dVar68);
                dVar64 = dVar64 / dVar68;
                dVar65 = dVar65 / dVar68;
                dVar66 = dVar66 / dVar68;
                dVar71 = dVar67 / dVar68;
              }
            }
            else {
              auVar42._0_8_ = auVar56._0_8_ * 0.5;
              auVar42._8_8_ = 0;
              auVar39 = auVar56;
              dVar68 = (double)func_0x0001034627b8(auVar42);
              dVar66 = (dVar66 * dVar68) / auVar56._0_8_;
              dVar71 = auVar39._0_8_;
              dVar64 = (dVar64 * dVar68) / auVar56._0_8_;
              dVar65 = (dVar65 * dVar68) / auVar56._0_8_;
              dVar68 = dVar64 * dVar64 + dVar66 * dVar66 + dVar65 * dVar65 + dVar71 * dVar71;
              if (0.0 < dVar68) {
                dVar68 = SQRT(dVar68);
                dVar64 = dVar64 / dVar68;
                dVar65 = dVar65 / dVar68;
                dVar66 = dVar66 / dVar68;
                dVar71 = dVar71 / dVar68;
              }
            }
            pdVar18 = local_9c0;
            pdVar29 = local_9c8;
            dVar68 = ((-local_980 * dVar64 + dStack_968 * dVar71) - dStack_978 * dVar65) -
                     local_970 * dVar66;
            dVar75 = (local_980 * dVar71 + dStack_968 * dVar64 + dStack_978 * dVar66) -
                     local_970 * dVar65;
            dVar69 = (dStack_978 * dVar71 + dStack_968 * dVar65 + local_970 * dVar64) -
                     local_980 * dVar66;
            local_970 = (local_970 * dVar71 + dStack_968 * dVar66 + local_980 * dVar65) -
                        dStack_978 * dVar64;
            dVar71 = dVar75 * dVar75 + local_970 * local_970 + dVar69 * dVar69 + dVar68 * dVar68;
            if (0.0 < dVar71) {
              dVar71 = SQRT(dVar71);
              dVar75 = dVar75 / dVar71;
              dVar69 = dVar69 / dVar71;
              local_970 = local_970 / dVar71;
              dVar68 = dVar68 / dVar71;
            }
            dVar64 = dVar73 * local_970 + -dVar72 * dVar69;
            dVar65 = dVar72 * dVar75 + dVar53 * local_970;
            dVar66 = -(dVar69 * dVar53) + -dVar73 * dVar75;
            dVar64 = dVar64 + dVar64;
            dVar65 = dVar65 + dVar65;
            dVar66 = dVar66 + dVar66;
            dVar71 = dVar53 + dVar64 * dVar68 + -local_970 * dVar65 + dVar66 * dVar69;
            dVar73 = -dVar73 + dVar65 * dVar68 + -(dVar75 * dVar66) + local_970 * dVar64;
            dVar53 = (dVar68 * dVar66 - dVar72) + -dVar69 * dVar64 + dVar75 * dVar65;
            lVar17 = lVar25;
            local_980 = dVar75;
            dStack_978 = dVar69;
            dStack_968 = dVar68;
            if (local_9b8 <= local_9c0) break;
            local_9c0[1] = dVar73;
            *local_9c0 = dVar71;
            local_9c0[2] = dVar53;
            local_9c0 = local_9c0 + 3;
            if (lVar25 <= lVar11) goto LAB_1027bbbe8;
          }
          lVar26 = (long)local_9c0 - (long)local_9c8 >> 3;
          uVar34 = lVar26 * -0x5555555555555555 + 1;
          if (0xaaaaaaaaaaaaaaa < uVar34) {
            func_0x000100023e9c(&local_9c8);
            goto LAB_1027bc2a8;
          }
          lVar20 = (long)local_9b8 - (long)local_9c8 >> 3;
          uVar28 = lVar20 * 0x5555555555555556;
          if (uVar28 < uVar34 || uVar28 - uVar34 == 0) {
            uVar28 = uVar34;
          }
          if (0x555555555555554 < (ulong)(lVar20 * -0x5555555555555555)) {
            uVar28 = 0xaaaaaaaaaaaaaaa;
          }
          if (uVar28 == 0) {
            lVar20 = 0;
          }
          else {
            if (0xaaaaaaaaaaaaaaa < uVar28) {
              func_0x00010002db08();
              goto LAB_1027bc2a8;
            }
            lVar20 = func_0x000103462620(uVar28 * 0x18);
          }
          pdVar24 = (double *)(lVar20 + lVar26 * 8);
          pdVar24[1] = dVar73;
          *pdVar24 = dVar71;
          pdVar24[2] = dVar53;
          pdVar21 = pdVar24;
          pdVar22 = pdVar18;
          for (; pdVar18 != pdVar29; pdVar18 = pdVar18 + -3) {
            auVar39 = *(undefined1 (*) [16])(pdVar18 + -3);
            pdVar21[-1] = pdVar18[-1];
            pdVar21[-2] = auVar39._8_8_;
            pdVar21[-3] = auVar39._0_8_;
            pdVar21 = pdVar21 + -3;
            pdVar22 = local_9c8;
          }
          local_9b8 = (double *)(lVar20 + uVar28 * 0x18);
          local_9c8 = pdVar21;
          if (pdVar22 != (double *)0x0) {
            func_0x000103462518(pdVar22);
          }
          local_9c0 = pdVar24 + 3;
        } while (lVar11 < lVar25);
      }
LAB_1027bbbe8:
      auVar43._8_8_ = 0;
      auVar43._0_8_ = local_428[0];
      dVar35 = auVar38._0_8_ / dVar35;
      auVar57._8_8_ = 0;
      auVar57._0_8_ = dVar35;
      func_0x0001027d3d7c(auVar43,auVar57,auStack_250,auStack_268);
      func_0x000103463454(auStack_238,0x103bd4080,0x30);
      uVar32 = (int)((ulong)((long)local_9a8 - (long)local_9b0) >> 3) * -0x55555555;
      while (uVar32 = uVar32 - 1, -1 < (int)uVar32) {
        auVar44._8_8_ = 0;
        auVar44._0_8_ = local_428[0];
        auVar58._8_8_ = 0;
        auVar58._0_8_ = dVar35;
        func_0x0001027d24e4(auVar44,auVar58,local_9b0 + (ulong)uVar32 * 3,3,auStack_250,auStack_268,
                            auStack_238,local_9b0 + (ulong)uVar32 * 3);
      }
      if (local_990 != local_998) {
        lVar17 = 0;
        uVar19 = 0;
        do {
          auVar45._8_8_ = 0;
          auVar45._0_8_ = local_428[0];
          auVar59._8_8_ = 0;
          auVar59._0_8_ = dVar35;
          func_0x0001027d24e4(auVar45,auVar59,(long)local_998 + lVar17,3,auStack_250,auStack_268,
                              auStack_238,(long)local_998 + lVar17);
          uVar19 = uVar19 + 1;
          uVar34 = ((long)local_990 - (long)local_998 >> 3) * -0x5555555555555555;
          lVar17 = lVar17 + 0x18;
        } while (uVar19 <= uVar34 && uVar34 - uVar19 != 0);
      }
      if (local_9c0 != local_9c8) {
        lVar17 = 0;
        uVar19 = 0;
        do {
          auVar46._8_8_ = 0;
          auVar46._0_8_ = local_428[0];
          auVar60._8_8_ = 0;
          auVar60._0_8_ = dVar35;
          func_0x0001027d24e4(auVar46,auVar60,(long)local_9c8 + lVar17,3,auStack_250,auStack_268,
                              auStack_238,(long)local_9c8 + lVar17);
          uVar19 = uVar19 + 1;
          uVar34 = ((long)local_9c0 - (long)local_9c8 >> 3) * -0x5555555555555555;
          lVar17 = lVar17 + 0x18;
        } while (uVar19 <= uVar34 && uVar34 - uVar19 != 0);
      }
      auVar47._0_8_ = local_9c0[-3];
      auVar47._8_8_ = 0;
      func_0x0001027d3fbc(auVar47,auStack_250,auStack_268,auStack_238);
      auVar48._0_8_ = local_9c0[-2];
      auVar48._8_8_ = 0;
      func_0x0001027d3fbc(auVar48,auStack_250,auStack_268,auStack_228);
      auVar49._0_8_ = local_9c0[-1];
      auVar49._8_8_ = 0;
      func_0x0001027d3fbc(auVar49,auStack_250,auStack_268,auStack_218);
      uVar32 = (int)((ulong)((long)local_9c0 - (long)local_9c8) >> 3) * -0x55555555;
      while (pdVar18 = local_998, uVar32 = uVar32 - 1, -1 < (int)uVar32) {
        auVar50._8_8_ = 0;
        auVar50._0_8_ = local_428[0];
        auVar61._8_8_ = 0;
        auVar61._0_8_ = dVar35;
        func_0x0001027d24e4(auVar50,auVar61,local_9c8 + (ulong)uVar32 * 3,3,auStack_250,auStack_268,
                            auStack_238,local_9c8 + (ulong)uVar32 * 3);
      }
      uVar32 = (int)((ulong)((long)local_990 - (long)local_998) >> 3) * -0x55555555;
      while (uVar32 = uVar32 - 1, -1 < (int)uVar32) {
        auVar51._8_8_ = 0;
        auVar51._0_8_ = local_428[0];
        auVar62._8_8_ = 0;
        auVar62._0_8_ = dVar35;
        func_0x0001027d24e4(auVar51,auVar62,pdVar18 + (ulong)uVar32 * 3,3,auStack_250,auStack_268,
                            auStack_238,pdVar18 + (ulong)uVar32 * 3);
      }
      if (local_9a8 != local_9b0) {
        lVar17 = 0;
        uVar19 = 0;
        do {
          auVar52._8_8_ = 0;
          auVar52._0_8_ = local_428[0];
          auVar63._8_8_ = 0;
          auVar63._0_8_ = dVar35;
          func_0x0001027d24e4(auVar52,auVar63,(long)local_9b0 + lVar17,3,auStack_250,auStack_268,
                              auStack_238,(long)local_9b0 + lVar17);
          uVar19 = uVar19 + 1;
          uVar34 = ((long)local_9a8 - (long)local_9b0 >> 3) * -0x5555555555555555;
          lVar17 = lVar17 + 0x18;
        } while (uVar19 <= uVar34 && uVar34 - uVar19 != 0);
      }
      dVar35 = dRam0000000103c12258;
      dVar67 = _DAT_103c12250;
      uVar19 = *(ulong *)(param_1 + 0xa8);
      if (uVar19 != 0) {
        uVar34 = 0;
        dVar53 = 0.0;
        dVar70 = DAT_1037c4eb0._8_8_;
        dVar37 = (double)DAT_1037c4eb0;
        pdVar18 = local_998;
        dVar71 = dVar37;
        dVar73 = dVar70;
        dVar36 = 0.0;
        do {
          dVar64 = pdVar18[2];
          dVar65 = -(dVar71 * pdVar18[1]) + dVar64 * dVar36;
          dVar68 = -(dVar53 * dVar64) + *pdVar18 * dVar71;
          dVar75 = -(dVar36 * *pdVar18) + pdVar18[1] * dVar53;
          dVar65 = dVar65 + dVar65;
          dVar68 = dVar68 + dVar68;
          dVar75 = dVar75 + dVar75;
          dVar72 = *pdVar18 + dVar65 * dVar73 + -dVar71 * dVar68 + dVar75 * dVar36;
          dVar66 = pdVar18[1] + dVar68 * dVar73 + -(dVar53 * dVar75) + dVar65 * dVar71;
          dVar64 = dVar64 + dVar75 * dVar73 + -dVar36 * dVar65 + dVar68 * dVar53;
          dVar65 = dVar64 * dVar64 + dVar72 * dVar72 + dVar66 * dVar66;
          if (0.0 < dVar65) {
            dVar65 = SQRT(dVar65);
            dVar72 = dVar72 / dVar65;
            dVar66 = dVar66 / dVar65;
            dVar64 = dVar64 / dVar65;
          }
          dVar75 = SQRT((dVar64 + 1.0) * 0.5);
          dVar64 = dVar37;
          dVar65 = dVar70;
          dVar68 = 0.0;
          if (1e-06 < dVar75) {
            dVar64 = (dVar72 * dVar67) / dVar75;
            dVar65 = (dVar66 * dVar35) / dVar75;
            dVar68 = dVar75;
          }
          dVar75 = ((-(dVar65 * dVar53) + dVar73 * dVar68) - dVar36 * dVar64) + dVar71 * -0.0;
          dVar72 = dVar71 * dVar64;
          dVar66 = (dVar73 * dVar64 + dVar36 * dVar68 + dVar53 * 0.0) - dVar71 * dVar65;
          dVar71 = (dVar73 * 0.0 + dVar71 * dVar68 + dVar36 * dVar65) - dVar53 * dVar64;
          dVar53 = dVar73 * dVar65 + dVar53 * dVar68 + dVar72 + dVar36 * -0.0;
          dVar73 = dVar71 * dVar71 + dVar53 * dVar53 + dVar75 * dVar75 + dVar66 * dVar66;
          if (0.0 < dVar73) {
            dVar73 = SQRT(dVar73);
            dVar53 = dVar53 / dVar73;
            dVar66 = dVar66 / dVar73;
            dVar71 = dVar71 / dVar73;
            dVar75 = dVar75 / dVar73;
          }
          uVar28 = uVar34 + *(long *)(param_1 + 0xb0);
          uVar30 = *(ulong *)(param_1 + 0xa0);
          uVar2 = 0;
          if (uVar30 != 0) {
            uVar2 = uVar28 / uVar30;
          }
          pdVar29 = (double *)(*(long *)(param_1 + 0x98) + (uVar28 - uVar2 * uVar30) * 0x20);
          dVar73 = *pdVar29;
          dVar36 = pdVar29[1];
          dVar64 = pdVar29[2];
          dVar72 = pdVar29[3];
          *pdVar29 = -(-dVar53 * dVar64) - (dVar72 * dVar66 + dVar36 * dVar75 + dVar73 * dVar71);
          pdVar29[1] = (dVar72 * dVar53 + dVar73 * dVar75 + dVar64 * dVar66) - dVar36 * dVar71;
          pdVar29[2] = -(-dVar66 * dVar73) - (dVar72 * dVar71 + dVar64 * dVar75 + dVar36 * dVar53);
          pdVar29[3] = ((-(dVar53 * dVar73) + dVar72 * dVar75) - dVar36 * dVar66) - dVar64 * dVar71;
          dVar64 = pdVar29[1];
          dVar72 = *pdVar29;
          dVar73 = pdVar29[2];
          dVar36 = pdVar29[3];
          dVar65 = dVar72 * dVar72 + dVar73 * dVar73 + dVar64 * dVar64 + dVar36 * dVar36;
          if (0.0 < dVar65) {
            dVar65 = SQRT(dVar65);
            pdVar29[1] = dVar64 / dVar65;
            *pdVar29 = dVar72 / dVar65;
            pdVar29[3] = dVar36 / dVar65;
            pdVar29[2] = dVar73 / dVar65;
            uVar19 = *(ulong *)(param_1 + 0xa8);
          }
          uVar34 = uVar34 + 1;
          pdVar18 = pdVar18 + 3;
          dVar73 = dVar75;
          dVar36 = dVar66;
        } while (uVar34 < uVar19);
      }
      if (local_9c8 != (double *)0x0) {
        func_0x000103462518();
      }
      if (local_9b0 != (double *)0x0) {
        local_9a8 = local_9b0;
        func_0x000103462518();
      }
      if (local_998 != (double *)0x0) {
        func_0x000103462518();
      }
      if (local_960 != (double *)0x0) {
        func_0x000103462518();
      }
      if (local_948 != (undefined8 *)0x0) {
        local_940 = local_948;
        func_0x000103462518();
      }
      if (local_370 != (double *)0x0) {
        func_0x000103462518();
      }
      if (local_358 != (double *)0x0) {
        func_0x000103462518();
      }
      if (local_340 != (double *)0x0) {
        func_0x000103462518();
      }
      if (local_328 != (double *)0x0) {
        local_320 = local_328;
        func_0x000103462518();
      }
      if (*_DAT_103fed798 == local_b8) {
        return 0;
      }
      func_0x0001034627dc();
    }
    else if ((uVar34 & 0xffffffff) >> 0x1f == 0) {
      lVar25 = lVar11 * 0x18;
      local_328 = (double *)func_0x000103462620(lVar25);
      local_318 = local_328 + (long)iVar15 * 3;
      local_320 = local_328;
      local_340 = (double *)func_0x000103462620(lVar11 * 0x48);
      local_330 = local_340 + (long)iVar15 * 9;
      local_350 = (double *)0x0;
      local_358 = (double *)0x0;
      local_348 = (double *)0x0;
      local_368 = (double *)0x0;
      local_370 = (double *)0x0;
      local_360 = (double *)0x0;
      local_338 = local_340;
      local_358 = (double *)func_0x000103462620(lVar25);
      local_348 = local_358 + (long)iVar15 * 3;
      local_350 = local_358;
      local_368 = (double *)func_0x000103462620(lVar25);
      local_360 = local_368 + (long)iVar15 * 3;
      goto LAB_1027b9374;
    }
    func_0x000100023e9c(&local_328);
  }
LAB_1027bc2a8:
                    /* WARNING: Does not return */
  pcVar9 = (code *)SoftwareBreakpoint(1,0x1027bc2ac);
  (*pcVar9)();
}

