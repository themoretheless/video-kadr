
undefined8
entry(long *param_1,long *param_2,int *param_3,undefined4 *param_4,int *param_5,undefined8 *param_6,
     undefined4 param_7,undefined1 param_8)

{
  undefined8 uVar1;
  code *pcVar2;
  long lVar3;
  int iVar4;
  long lVar5;
  long lVar6;
  undefined8 *puVar7;
  ulong uVar8;
  double *pdVar9;
  ulong *puVar10;
  double *pdVar11;
  double *pdVar12;
  ulong uVar13;
  long *plVar14;
  long *plVar15;
  double *pdVar16;
  ulong uVar17;
  long lVar18;
  double *pdVar19;
  double dVar20;
  double dVar21;
  double dVar22;
  double local_110;
  double dStack_108;
  double local_100;
  double dStack_f8;
  undefined8 local_f0 [2];
  long local_e0;
  long *local_d8;
  long *local_d0;
  long *local_c8;
  undefined8 local_c0 [2];
  undefined8 local_b0;
  double local_a8;
  double local_a0;
  double dStack_98;
  double local_90;
  
  *(undefined1 *)(param_1 + 0x1a) = param_8;
  if (param_2[1] == 0) {
    return 0xffffffff;
  }
  *(undefined4 *)(param_1 + 0x13) = *param_4;
  *(undefined4 *)((long)param_1 + 0x9c) = param_4[1];
  lVar5 = *param_2;
  *(undefined4 *)(param_1 + 0x12) = *(undefined4 *)(lVar5 + 0x10);
  *(undefined4 *)((long)param_1 + 0x94) = *(undefined4 *)(lVar5 + 0x14);
  lVar5 = *param_1;
  lVar3 = param_1[1];
  if (lVar3 != lVar5) {
    uVar17 = 0;
    lVar18 = 8;
    do {
      plVar14 = *(long **)(lVar5 + lVar18);
      if (plVar14 != (long *)0x0) {
        plVar15 = plVar14 + 1;
        *plVar15 = *plVar15 + 1;
        LOAcquire();
        lVar5 = *plVar15;
        *plVar15 = lVar5 + -1;
        LORelease();
        if (lVar5 == 0) {
          (**(code **)(*plVar14 + 0x10))(plVar14);
          func_0x000103462194(plVar14);
        }
      }
      uVar17 = uVar17 + 1;
      lVar5 = *param_1;
      lVar3 = param_1[1];
      lVar18 = lVar18 + 0x10;
      lVar6 = lVar3;
    } while (uVar17 < (ulong)(lVar3 - lVar5 >> 4));
    for (; lVar6 != lVar5; lVar6 = lVar6 + -0x10) {
      plVar14 = *(long **)(lVar6 + -8);
      if (plVar14 != (long *)0x0) {
        LOAcquire();
        lVar3 = plVar14[1];
        plVar14[1] = lVar3 + -1;
        LORelease();
        if (lVar3 == 0) {
          (**(code **)(*plVar14 + 0x10))(plVar14);
          func_0x000103462194(plVar14);
        }
      }
      lVar3 = lVar5;
    }
  }
  lVar5 = param_1[3];
  param_1[1] = lVar3;
  lVar3 = param_1[4];
  if (param_1[4] != lVar5) {
    uVar17 = 0;
    lVar3 = 8;
    do {
      plVar14 = *(long **)(lVar5 + lVar3);
      if (plVar14 != (long *)0x0) {
        plVar15 = plVar14 + 1;
        *plVar15 = *plVar15 + 1;
        LOAcquire();
        lVar5 = *plVar15;
        *plVar15 = lVar5 + -1;
        LORelease();
        if (lVar5 == 0) {
          (**(code **)(*plVar14 + 0x10))(plVar14);
          func_0x000103462194(plVar14);
        }
      }
      uVar17 = uVar17 + 1;
      lVar5 = param_1[3];
      lVar18 = param_1[4];
      lVar3 = lVar3 + 0x10;
    } while (uVar17 < (ulong)(lVar18 - lVar5 >> 4));
    if (lVar18 == lVar5) {
      lVar6 = param_1[0xe];
      param_1[4] = lVar18;
      lVar5 = param_1[0xf];
      uVar17 = lVar5 - lVar6;
      goto joined_r0x000101b540d4;
    }
    do {
      plVar14 = *(long **)(lVar18 + -8);
      if (plVar14 != (long *)0x0) {
        LOAcquire();
        lVar3 = plVar14[1];
        plVar14[1] = lVar3 + -1;
        LORelease();
        if (lVar3 == 0) {
          (**(code **)(*plVar14 + 0x10))(plVar14);
          func_0x000103462194(plVar14);
        }
      }
      lVar18 = lVar18 + -0x10;
      lVar3 = lVar5;
    } while (lVar18 != lVar5);
  }
  lVar6 = param_1[0xe];
  param_1[4] = lVar3;
  lVar5 = param_1[0xf];
  uVar17 = lVar5 - lVar6;
joined_r0x000101b540d4:
  if (uVar17 == 0) {
    func_0x0001008c9bcc(param_1 + 0xe,1);
    puVar7 = (undefined8 *)param_1[0xe];
    uVar1 = *param_6;
    lVar5 = param_6[1];
  }
  else {
    if (0x10 < uVar17) {
      for (; lVar5 != lVar6 + 0x10; lVar5 = lVar5 + -0x10) {
        plVar14 = *(long **)(lVar5 + -8);
        if (plVar14 != (long *)0x0) {
          LOAcquire();
          lVar3 = plVar14[1];
          plVar14[1] = lVar3 + -1;
          LORelease();
          if (lVar3 == 0) {
            (**(code **)(*plVar14 + 0x10))(plVar14);
            func_0x000103462194(plVar14);
          }
        }
      }
      param_1[0xf] = lVar6 + 0x10;
    }
    puVar7 = (undefined8 *)param_1[0xe];
    uVar1 = *param_6;
    lVar5 = param_6[1];
  }
  if (lVar5 != 0) {
    *(long *)(lVar5 + 8) = *(long *)(lVar5 + 8) + 1;
  }
  plVar14 = (long *)puVar7[1];
  *puVar7 = uVar1;
  puVar7[1] = lVar5;
  if (plVar14 != (long *)0x0) {
    LOAcquire();
    lVar5 = plVar14[1];
    plVar14[1] = lVar5 + -1;
    LORelease();
    if (lVar5 == 0) {
      (**(code **)(*plVar14 + 0x10))(plVar14);
      func_0x000103462194(plVar14);
    }
  }
  local_b0 = 0;
  local_a8 = 6.283185307179586 / (double)(long)*param_3;
  local_a0 = 3.141592653589793 / (double)(param_3[1] + -1);
  dStack_98 = (double)(long)*param_3 / 6.283185307179586;
  local_90 = (double)(param_3[1] + -1) / 3.141592653589793;
  puVar10 = (ulong *)(param_1 + 6);
  param_1[7] = *puVar10;
  if (0 < param_2[1]) {
    lVar5 = 0;
    do {
      func_0x0001027ab568(local_c0,*param_2 + lVar5 * 0xc0);
      func_0x0001027aa800(&local_d0,local_c0,*(undefined4 *)(*param_2 + lVar5 * 0xc0 + 0x4c));
      if (*(int *)(*param_2 + lVar5 * 0xc0 + 0xb4) != 0) {
        (**(code **)(*local_d0 + 0x40))(local_d0,1);
      }
      (**(code **)(*local_d0 + 0x18))((double)(long)*param_5,(double)(long)param_5[1]);
      dVar20 = (double)func_0x000101b55414(*(undefined4 *)(*param_2 + lVar5 * 0xc0 + 0x48));
      dVar22 = (dVar20 * 3.141592653589793) / 180.0;
      dVar20 = dVar22 * 1.2;
      (**(code **)(*local_d0 + 0x48))(dVar20);
      puVar7 = (undefined8 *)param_1[4];
      if (puVar7 < (undefined8 *)param_1[5]) {
        *puVar7 = local_d0;
        puVar7[1] = local_c8;
        if (local_c8 != (long *)0x0) {
          local_c8[1] = local_c8[1] + 1;
        }
        puVar7 = puVar7 + 2;
      }
      else {
        puVar7 = (undefined8 *)func_0x0001000332f4(param_1 + 3,&local_d0);
      }
      param_1[4] = (long)puVar7;
      plVar14 = (long *)func_0x000103462620(0x1a0);
      plVar14[2] = 0;
      *plVar14 = 0x1040b0e50;
      plVar14[1] = 0;
      local_e0 = func_0x000101b4fb18(plVar14 + 4);
      local_d8 = plVar14;
      func_0x000101b55624(dVar22,local_e0,&local_d0,&local_b0,param_5,param_4,param_7);
      uVar1 = *param_6;
      plVar14 = (long *)param_6[1];
      if (plVar14 != (long *)0x0) {
        plVar15 = plVar14 + 1;
        *plVar15 = *plVar15 + 1;
        *plVar15 = *plVar15 + 1;
      }
      plVar15 = *(long **)(local_e0 + 200);
      *(undefined8 *)(local_e0 + 0xc0) = uVar1;
      *(long **)(local_e0 + 200) = plVar14;
      if (plVar15 != (long *)0x0) {
        LOAcquire();
        lVar3 = plVar15[1];
        plVar15[1] = lVar3 + -1;
        LORelease();
        if (lVar3 == 0) {
          (**(code **)(*plVar15 + 0x10))(plVar15);
          func_0x000103462194(plVar15);
        }
      }
      if (plVar14 != (long *)0x0) {
        LOAcquire();
        lVar3 = plVar14[1];
        plVar14[1] = lVar3 + -1;
        LORelease();
        if (lVar3 == 0) {
          (**(code **)(*plVar14 + 0x10))(plVar14);
          func_0x000103462194(plVar14);
        }
      }
      lVar3 = local_e0;
      func_0x0001013276f8(local_f0,local_c0);
      lVar18 = *param_2 + lVar5 * 0xc0;
      func_0x000101b55808(dVar20,lVar3,local_f0,*(undefined4 *)(lVar18 + 0x4c),
                          *(undefined4 *)(lVar18 + 0xb4));
      func_0x000103462eb4(local_f0[0]);
      iVar4 = func_0x000101b510fc(local_e0,*(undefined4 *)((long)param_1 + 0xcc),*param_3,param_3[1]
                                 );
      plVar14 = local_d8;
      if (iVar4 != 0) {
        if (local_d8 != (long *)0x0) {
          LOAcquire();
          lVar5 = local_d8[1];
          local_d8[1] = lVar5 + -1;
          LORelease();
          if (lVar5 == 0) {
            (**(code **)(*local_d8 + 0x10))(local_d8);
            func_0x000103462194(plVar14);
          }
        }
        if (local_c8 != (long *)0x0) {
          LOAcquire();
          lVar5 = local_c8[1];
          local_c8[1] = lVar5 + -1;
          LORelease();
          if (lVar5 == 0) {
            (**(code **)(*local_c8 + 0x10))(local_c8);
            func_0x000103462194(local_c8);
          }
        }
        func_0x000103462eb4(local_c0[0]);
        return 0xffffffff;
      }
      plVar14 = (long *)param_1[1];
      if (plVar14 < (long *)param_1[2]) {
        *plVar14 = local_e0;
        plVar14[1] = (long)local_d8;
        if (local_d8 != (long *)0x0) {
          local_d8[1] = local_d8[1] + 1;
        }
        plVar14 = plVar14 + 2;
      }
      else {
        plVar14 = (long *)func_0x0001000332f4(param_1,&local_e0);
      }
      param_1[1] = (long)plVar14;
      func_0x000101032fdc(&local_110,*param_2 + lVar5 * 0xc0 + 0x50);
      if ((lVar5 == 0) && (param_2[1] == 2)) {
        dVar22 = (local_110 * -0.0 + dStack_f8 * 0.0 + dStack_108 * -0.0) - local_100;
        dVar20 = local_100 * -0.0;
        dVar21 = (dStack_108 * 0.0 + dStack_f8 * 0.0 + local_100 * 0.0) - local_110;
        local_100 = dStack_f8 + local_100 * 0.0 + local_110 * 0.0 + dStack_108 * -0.0;
        local_110 = dStack_108 + local_110 * 0.0 + dStack_f8 * 0.0 + dVar20;
        dStack_108 = dVar21;
        dStack_f8 = dVar22;
      }
      pdVar19 = (double *)param_1[7];
      if (pdVar19 < (double *)param_1[8]) {
        pdVar19[1] = dStack_108;
        *pdVar19 = local_110;
        pdVar19[3] = dStack_f8;
        pdVar19[2] = local_100;
        pdVar16 = pdVar19 + 4;
      }
      else {
        pdVar16 = (double *)*puVar10;
        lVar3 = (long)pdVar19 - (long)pdVar16 >> 5;
        uVar17 = lVar3 + 1;
        if (uVar17 >> 0x3b != 0) {
          func_0x000100023e9c(puVar10);
LAB_101b547b4:
                    /* WARNING: Does not return */
          pcVar2 = (code *)SoftwareBreakpoint(1,0x101b547b8);
          (*pcVar2)();
        }
        uVar8 = param_1[8] - (long)pdVar16;
        uVar13 = (long)uVar8 >> 4;
        if (uVar13 <= uVar17) {
          uVar13 = uVar17;
        }
        if (0x7fffffffffffffdf < uVar8) {
          uVar13 = 0x7ffffffffffffff;
        }
        if (uVar13 == 0) {
          lVar18 = 0;
          pdVar9 = (double *)(lVar3 * 0x20);
          pdVar9[1] = dStack_108;
          *pdVar9 = local_110;
          pdVar9[3] = dStack_f8;
          pdVar9[2] = local_100;
          pdVar11 = pdVar9;
          if (pdVar19 == pdVar16) goto LAB_101b54618;
LAB_101b545e0:
          do {
            dVar20 = pdVar19[-4];
            dVar21 = pdVar19[-1];
            dVar22 = pdVar19[-2];
            pdVar12 = pdVar11 + -4;
            pdVar11[-3] = pdVar19[-3];
            *pdVar12 = dVar20;
            pdVar11[-1] = dVar21;
            pdVar11[-2] = dVar22;
            pdVar19 = pdVar19 + -4;
            pdVar11 = pdVar12;
          } while (pdVar19 != pdVar16);
          pdVar19 = (double *)*puVar10;
          pdVar16 = pdVar9 + 4;
          param_1[6] = (long)pdVar12;
          param_1[7] = (long)pdVar16;
          param_1[8] = lVar18 + uVar13 * 0x20;
        }
        else {
          if (uVar13 >> 0x3b != 0) {
            func_0x00010002db08();
            goto LAB_101b547b4;
          }
          lVar18 = func_0x000103462620(uVar13 << 5);
          pdVar9 = (double *)(lVar18 + lVar3 * 0x20);
          pdVar9[1] = dStack_108;
          *pdVar9 = local_110;
          pdVar9[3] = dStack_f8;
          pdVar9[2] = local_100;
          pdVar11 = pdVar9;
          if (pdVar19 != pdVar16) goto LAB_101b545e0;
LAB_101b54618:
          pdVar16 = pdVar9 + 4;
          param_1[6] = (long)pdVar9;
          param_1[7] = (long)pdVar16;
          param_1[8] = lVar18 + uVar13 * 0x20;
        }
        if (pdVar19 != (double *)0x0) {
          func_0x000103462518(pdVar19);
        }
      }
      plVar15 = local_d8;
      param_1[7] = (long)pdVar16;
      plVar14 = local_c8;
      if (local_d8 != (long *)0x0) {
        LOAcquire();
        lVar3 = local_d8[1];
        local_d8[1] = lVar3 + -1;
        LORelease();
        if (lVar3 == 0) {
          (**(code **)(*local_d8 + 0x10))(local_d8);
          func_0x000103462194(plVar15);
          plVar14 = local_c8;
        }
      }
      local_c8 = plVar14;
      if (plVar14 != (long *)0x0) {
        LOAcquire();
        lVar3 = plVar14[1];
        plVar14[1] = lVar3 + -1;
        LORelease();
        if (lVar3 == 0) {
          (**(code **)(*plVar14 + 0x10))(plVar14);
          func_0x000103462194(plVar14);
        }
      }
      func_0x000103462eb4(local_c0[0]);
      lVar5 = lVar5 + 1;
    } while (lVar5 < param_2[1]);
  }
  *(int *)(param_1 + 0x12) = *param_5;
  *(int *)((long)param_1 + 0x94) = param_5[1];
  *(int *)(param_1 + 0x14) = *param_3;
  *(int *)((long)param_1 + 0xa4) = param_3[1];
  return 0;
}

