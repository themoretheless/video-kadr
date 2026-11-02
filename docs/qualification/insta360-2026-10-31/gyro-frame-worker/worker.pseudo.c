
/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

undefined8 entry(undefined8 *param_1,long *param_2)

{
  ulong *puVar1;
  ulong *puVar2;
  byte bVar3;
  int iVar4;
  char cVar5;
  char cVar6;
  ushort uVar7;
  undefined8 uVar8;
  byte bVar9;
  uint uVar10;
  int iVar11;
  undefined8 *puVar12;
  undefined8 *puVar13;
  long lVar14;
  byte bVar15;
  ulong uVar16;
  ulong uVar17;
  ulong uVar18;
  ulong uVar19;
  ulong uVar20;
  undefined8 uVar21;
  long *plVar22;
  long *plVar23;
  ulong uVar24;
  int *piVar25;
  byte bVar26;
  undefined8 local_108;
  undefined8 local_100;
  undefined8 local_f0;
  ulong auStack_e8 [15];
  long local_70;
  
  local_70 = *_DAT_103fed798;
  uVar7 = *(ushort *)((long)param_2 + 10);
  if ((uVar7 != 0xffff) && (uVar10 = func_0x00010345e2f4(param_2), uVar7 != uVar10)) {
    func_0x00010345e2f4(param_2);
  }
  if (param_1[0xd] == 0) {
    param_1[0xd] = 1;
    uVar10 = func_0x00010345e2f4(param_2);
    if ((uVar10 != *(ushort *)(param_2 + 1)) && (1 < *(int *)(param_1[0xc] + 8))) {
      *(undefined1 *)(param_1[0xc] + 0x18) = 1;
      bVar15 = *(byte *)((long)param_1 + 0x74);
      if (bVar15 < 2) {
        bVar15 = 1;
      }
      *(byte *)((long)param_1 + 0x74) = bVar15 + 1;
    }
  }
  uVar16 = param_1[9];
  uVar17 = (ulong)(*(int *)(param_1 + 8) - *(int *)((long)param_1 + 0x44));
  if (uVar16 < uVar17) {
    if ((ulong)param_1[0xd] < 2) {
      if ((param_1[0xd] == 0) || (*(char *)((long)param_1 + 0x74) == '\0')) goto LAB_101b51494;
      *(char *)((long)param_1 + 0x74) = *(char *)((long)param_1 + 0x74) + -1;
      param_1[0xd] = 0;
    }
    while( true ) {
      local_100 = 0;
      puVar12 = (undefined8 *)func_0x00010345e384(&local_100,0x80,param_2);
      puVar12[7] = 0;
      puVar12[6] = 0;
      puVar12[5] = 0;
      puVar12[4] = 0;
      puVar12[3] = 0;
      puVar12[2] = 0;
      puVar12[1] = 0;
      *puVar12 = 0x1040b0db8;
      uVar16 = param_1[9];
      piVar25 = (int *)(param_1 + 8);
      *(int *)(puVar12 + 8) = *piVar25;
      iVar11 = *(int *)((long)param_1 + 0x44);
      iVar11 = iVar11 + ((uint)(*piVar25 - iVar11) >> 1);
      *piVar25 = iVar11;
      *(int *)((long)puVar12 + 0x44) = iVar11;
      puVar12[9] = uVar16;
      uVar21 = param_1[10];
      puVar12[0xb] = param_1[0xb];
      puVar12[10] = uVar21;
      uVar16 = param_1[0xd];
      param_1[0xd] = uVar16 >> 1;
      puVar12[0xd] = uVar16 >> 1;
      *(undefined4 *)(puVar12 + 0xe) = 2;
      *(undefined1 *)((long)puVar12 + 0x74) = *(undefined1 *)((long)param_1 + 0x74);
      puVar12[0xf] = local_100;
      puVar13 = (undefined8 *)func_0x00010345e384(&local_100,0x20,param_2);
      *puVar13 = param_1[0xc];
      *(undefined4 *)(puVar13 + 1) = 2;
      puVar13[2] = local_100;
      *(undefined1 *)(puVar13 + 3) = 0;
      param_1[0xc] = puVar13;
      puVar12[0xc] = puVar13;
      func_0x00010345e360(puVar12,*param_2);
      uVar16 = param_1[9];
      uVar17 = (ulong)(*piVar25 - *(int *)((long)param_1 + 0x44));
      if (uVar17 <= uVar16) break;
      if ((ulong)param_1[0xd] < 2) {
        if ((param_1[0xd] == 0) || (*(char *)((long)param_1 + 0x74) == '\0')) break;
        *(char *)((long)param_1 + 0x74) = *(char *)((long)param_1 + 0x74) + -1;
        param_1[0xd] = 0;
      }
    }
  }
LAB_101b51494:
  if ((uVar16 < uVar17) && (bVar15 = *(byte *)((long)param_1 + 0x74), bVar15 != 0)) {
    uVar17 = 0;
    uVar16 = 0;
    local_100 = CONCAT44(local_100._4_4_,0x10000);
    auStack_e8[0] = param_1[9];
    local_f0 = param_1[8];
    bVar26 = 1;
    do {
      uVar21 = local_100;
      iVar11 = (int)uVar16;
      uVar24 = uVar16;
      bVar9 = bVar26;
      if (*(byte *)((long)&local_100 + uVar16 + 3) < bVar15) {
        puVar1 = auStack_e8 + uVar16 * 2 + -1;
        piVar25 = (int *)((long)auStack_e8 + uVar16 * 0x10 + -4);
        if (auStack_e8[uVar16 * 2] < (ulong)((long)(int)*puVar1 - (long)*piVar25)) {
          uVar24 = (ulong)(iVar11 + 1U & 7);
          puVar2 = auStack_e8 + uVar24 * 2 + -1;
          uVar18 = *puVar1;
          auStack_e8[uVar24 * 2] = auStack_e8[uVar16 * 2];
          *puVar2 = uVar18;
          iVar4 = *(int *)((long)auStack_e8 + uVar24 * 0x10 + -4);
          *(int *)puVar1 = (int)*puVar2;
          uVar18 = (ulong)&local_100 | 3;
          iVar4 = iVar4 + ((uint)((int)*puVar2 - iVar4) >> 1);
          *(int *)puVar2 = iVar4;
          *piVar25 = iVar4;
          uVar19 = auStack_e8[uVar24 * 2];
          bVar3 = *(char *)(uVar18 + uVar16) + 1;
          *(byte *)(uVar18 + uVar16) = bVar3;
          auStack_e8[uVar16 * 2] = uVar19;
          *(byte *)(uVar18 + uVar24) = bVar3;
          bVar9 = bVar26 + 1;
          if ((bVar26 < 7) && (bVar3 < bVar15)) {
            piVar25 = (int *)((long)auStack_e8 + uVar24 * 0x10 + -4);
            if (auStack_e8[uVar24 * 2] < (ulong)((long)(int)*puVar2 - (long)*piVar25)) {
              uVar19 = (ulong)(iVar11 + 2U & 7);
              puVar1 = auStack_e8 + uVar19 * 2 + -1;
              uVar18 = *puVar2;
              auStack_e8[uVar19 * 2] = auStack_e8[uVar24 * 2];
              *puVar1 = uVar18;
              iVar4 = *(int *)((long)auStack_e8 + uVar19 * 0x10 + -4);
              *(int *)puVar2 = (int)*puVar1;
              uVar18 = (ulong)&local_100 | 3;
              iVar4 = iVar4 + ((uint)((int)*puVar1 - iVar4) >> 1);
              *(int *)puVar1 = iVar4;
              *piVar25 = iVar4;
              uVar20 = auStack_e8[uVar19 * 2];
              bVar3 = *(char *)(uVar18 + uVar24) + 1;
              *(byte *)(uVar18 + uVar24) = bVar3;
              auStack_e8[uVar24 * 2] = uVar20;
              *(byte *)(uVar18 + uVar19) = bVar3;
              bVar9 = bVar26 + 2;
              uVar24 = uVar19;
              if ((bVar26 < 6) && (bVar3 < bVar15)) {
                piVar25 = (int *)((long)auStack_e8 + uVar19 * 0x10 + -4);
                if (auStack_e8[uVar19 * 2] < (ulong)((long)(int)*puVar1 - (long)*piVar25)) {
                  uVar24 = (ulong)(iVar11 + 3U & 7);
                  puVar2 = auStack_e8 + uVar24 * 2 + -1;
                  uVar18 = *puVar1;
                  auStack_e8[uVar24 * 2] = auStack_e8[uVar19 * 2];
                  *puVar2 = uVar18;
                  iVar4 = *(int *)((long)auStack_e8 + uVar24 * 0x10 + -4);
                  *(int *)puVar1 = (int)*puVar2;
                  uVar18 = (ulong)&local_100 | 3;
                  iVar4 = iVar4 + ((uint)((int)*puVar2 - iVar4) >> 1);
                  *(int *)puVar2 = iVar4;
                  *piVar25 = iVar4;
                  uVar20 = auStack_e8[uVar24 * 2];
                  bVar3 = *(char *)(uVar18 + uVar19) + 1;
                  *(byte *)(uVar18 + uVar19) = bVar3;
                  auStack_e8[uVar19 * 2] = uVar20;
                  *(byte *)(uVar18 + uVar24) = bVar3;
                  bVar9 = bVar26 + 3;
                  if ((bVar26 < 5) && (bVar3 < bVar15)) {
                    piVar25 = (int *)((long)auStack_e8 + uVar24 * 0x10 + -4);
                    if (auStack_e8[uVar24 * 2] < (ulong)((long)(int)*puVar2 - (long)*piVar25)) {
                      uVar19 = (ulong)(iVar11 + 4U & 7);
                      puVar1 = auStack_e8 + uVar19 * 2 + -1;
                      uVar18 = *puVar2;
                      auStack_e8[uVar19 * 2] = auStack_e8[uVar24 * 2];
                      *puVar1 = uVar18;
                      iVar4 = *(int *)((long)auStack_e8 + uVar19 * 0x10 + -4);
                      *(int *)puVar2 = (int)*puVar1;
                      uVar18 = (ulong)&local_100 | 3;
                      iVar4 = iVar4 + ((uint)((int)*puVar1 - iVar4) >> 1);
                      *(int *)puVar1 = iVar4;
                      *piVar25 = iVar4;
                      uVar20 = auStack_e8[uVar19 * 2];
                      bVar3 = *(char *)(uVar18 + uVar24) + 1;
                      *(byte *)(uVar18 + uVar24) = bVar3;
                      auStack_e8[uVar24 * 2] = uVar20;
                      *(byte *)(uVar18 + uVar19) = bVar3;
                      bVar9 = bVar26 + 4;
                      uVar24 = uVar19;
                      if ((bVar26 < 4) && (bVar3 < bVar15)) {
                        piVar25 = (int *)((long)auStack_e8 + uVar19 * 0x10 + -4);
                        if (auStack_e8[uVar19 * 2] < (ulong)((long)(int)*puVar1 - (long)*piVar25)) {
                          uVar24 = (ulong)(iVar11 + 5U & 7);
                          puVar2 = auStack_e8 + uVar24 * 2 + -1;
                          uVar18 = *puVar1;
                          auStack_e8[uVar24 * 2] = auStack_e8[uVar19 * 2];
                          *puVar2 = uVar18;
                          iVar4 = *(int *)((long)auStack_e8 + uVar24 * 0x10 + -4);
                          *(int *)puVar1 = (int)*puVar2;
                          uVar18 = (ulong)&local_100 | 3;
                          iVar4 = iVar4 + ((uint)((int)*puVar2 - iVar4) >> 1);
                          *(int *)puVar2 = iVar4;
                          *piVar25 = iVar4;
                          uVar20 = auStack_e8[uVar24 * 2];
                          bVar3 = *(char *)(uVar18 + uVar19) + 1;
                          *(byte *)(uVar18 + uVar19) = bVar3;
                          auStack_e8[uVar19 * 2] = uVar20;
                          *(byte *)(uVar18 + uVar24) = bVar3;
                          bVar9 = bVar26 + 5;
                          if ((bVar26 < 3) && (bVar3 < bVar15)) {
                            piVar25 = (int *)((long)auStack_e8 + uVar24 * 0x10 + -4);
                            if (auStack_e8[uVar24 * 2] <
                                (ulong)((long)(int)*puVar2 - (long)*piVar25)) {
                              uVar19 = (ulong)(iVar11 + 6U & 7);
                              puVar1 = auStack_e8 + uVar19 * 2 + -1;
                              uVar18 = *puVar2;
                              auStack_e8[uVar19 * 2] = auStack_e8[uVar24 * 2];
                              *puVar1 = uVar18;
                              iVar4 = *(int *)((long)auStack_e8 + uVar19 * 0x10 + -4);
                              *(int *)puVar2 = (int)*puVar1;
                              uVar18 = (ulong)&local_100 | 3;
                              iVar4 = iVar4 + ((uint)((int)*puVar1 - iVar4) >> 1);
                              *(int *)puVar1 = iVar4;
                              *piVar25 = iVar4;
                              uVar20 = auStack_e8[uVar19 * 2];
                              bVar3 = *(char *)(uVar18 + uVar24) + 1;
                              *(byte *)(uVar18 + uVar24) = bVar3;
                              auStack_e8[uVar24 * 2] = uVar20;
                              *(byte *)(uVar18 + uVar19) = bVar3;
                              bVar9 = bVar26 + 6;
                              uVar24 = uVar19;
                              if ((bVar26 < 2) && (bVar3 < bVar15)) {
                                piVar25 = (int *)((long)auStack_e8 + uVar19 * 0x10 + -4);
                                if (auStack_e8[uVar19 * 2] <
                                    (ulong)((long)(int)*puVar1 - (long)*piVar25)) {
                                  uVar24 = (ulong)(iVar11 - 1U & 7);
                                  puVar2 = auStack_e8 + uVar24 * 2 + -1;
                                  uVar18 = *puVar1;
                                  auStack_e8[uVar24 * 2] = auStack_e8[uVar19 * 2];
                                  *puVar2 = uVar18;
                                  iVar11 = *(int *)((long)auStack_e8 + uVar24 * 0x10 + -4);
                                  *(int *)puVar1 = (int)*puVar2;
                                  uVar18 = (ulong)&local_100 | 3;
                                  iVar11 = iVar11 + ((uint)((int)*puVar2 - iVar11) >> 1);
                                  *(int *)puVar2 = iVar11;
                                  *piVar25 = iVar11;
                                  auStack_e8[uVar19 * 2] = auStack_e8[uVar24 * 2];
                                  bVar3 = *(char *)(uVar18 + uVar19) + 1;
                                  *(byte *)(uVar18 + uVar19) = bVar3;
                                  *(byte *)(uVar18 + uVar24) = bVar3;
                                  bVar9 = bVar26 + 7;
                                  if ((bVar26 == 0) && (bVar3 < bVar15)) {
                                    piVar25 = (int *)((long)auStack_e8 + uVar24 * 0x10 + -4);
                                    if (auStack_e8[uVar24 * 2] <
                                        (ulong)((long)(int)*puVar2 - (long)*piVar25)) {
                                      puVar1 = auStack_e8 + uVar16 * 2 + -1;
                                      uVar18 = *puVar2;
                                      auStack_e8[uVar16 * 2] = auStack_e8[uVar24 * 2];
                                      *puVar1 = uVar18;
                                      iVar11 = *(int *)((long)auStack_e8 + uVar16 * 0x10 + -4);
                                      *(int *)puVar2 = (int)*puVar1;
                                      uVar18 = (ulong)&local_100 | 3;
                                      iVar11 = iVar11 + ((uint)((int)*puVar1 - iVar11) >> 1);
                                      *(int *)puVar1 = iVar11;
                                      *piVar25 = iVar11;
                                      auStack_e8[uVar24 * 2] = auStack_e8[uVar16 * 2];
                                      cVar5 = *(char *)(uVar18 + uVar24) + '\x01';
                                      *(char *)(uVar18 + uVar24) = cVar5;
                                      *(char *)(uVar18 + uVar16) = cVar5;
                                      uVar24 = uVar16;
                                      bVar9 = 8;
                                    }
                                  }
                                }
                              }
                            }
                          }
                        }
                      }
                    }
                  }
                }
              }
            }
          }
        }
      }
      bVar26 = bVar9;
      local_100 = CONCAT71(local_100._1_7_,(char)uVar24);
      uVar8 = local_100;
      local_100._3_5_ = SUB85(uVar21,3);
      local_100._0_2_ = (undefined2)uVar8;
      local_100._0_3_ = CONCAT12(bVar26,(undefined2)local_100);
      uVar16 = uVar24;
      do {
        if ((*(byte *)(param_1[0xc] + 0x18) & 1) == 0) {
LAB_101b51adc:
          func_0x000101b51cc8(param_1,(int)auStack_e8[uVar24 * 2 + -1],
                              *(undefined4 *)((long)auStack_e8 + uVar24 * 0x10 + -4));
          bVar26 = bVar26 - 1;
          local_100._0_3_ = CONCAT12(bVar26,(undefined2)local_100);
          uVar10 = (int)uVar24 - 1U & 7;
          uVar24 = (ulong)uVar10;
          local_100 = CONCAT71(local_100._1_7_,(char)uVar10);
LAB_101b51b00:
          uVar16 = uVar24;
          if (bVar26 == 0) goto LAB_101b51b34;
        }
        else {
          *(byte *)((long)param_1 + 0x74) = bVar15 + 1;
          if (bVar26 < 2) {
            if (((byte)(bVar15 + 1) <= *(byte *)((long)&local_100 + uVar24 + 3)) ||
               ((ulong)((long)(int)auStack_e8[uVar24 * 2 + -1] -
                       (long)*(int *)((long)auStack_e8 + uVar24 * 0x10 + -4)) <=
                auStack_e8[uVar24 * 2])) goto LAB_101b51adc;
            goto LAB_101b51b00;
          }
          cVar5 = *(char *)((long)&local_100 + uVar17 + 3);
          local_108 = 0;
          puVar12 = (undefined8 *)func_0x00010345e384(&local_108,0x80,param_2);
          puVar12[7] = 0;
          puVar12[6] = 0;
          puVar12[5] = 0;
          puVar12[4] = 0;
          puVar12[3] = 0;
          puVar12[2] = 0;
          puVar12[1] = 0;
          *puVar12 = 0x1040b0db8;
          uVar18 = auStack_e8[uVar17 * 2 + -1];
          puVar12[9] = auStack_e8[uVar17 * 2];
          puVar12[8] = uVar18;
          uVar21 = param_1[10];
          puVar12[0xb] = param_1[0xb];
          puVar12[10] = uVar21;
          uVar18 = param_1[0xd];
          param_1[0xd] = uVar18 >> 1;
          puVar12[0xd] = uVar18 >> 1;
          *(undefined4 *)(puVar12 + 0xe) = 2;
          cVar6 = *(char *)((long)param_1 + 0x74);
          puVar12[0xf] = local_108;
          *(char *)((long)puVar12 + 0x74) = cVar6 - cVar5;
          puVar13 = (undefined8 *)func_0x00010345e384(&local_108,0x20,param_2);
          *puVar13 = param_1[0xc];
          *(undefined4 *)(puVar13 + 1) = 2;
          puVar13[2] = local_108;
          *(undefined1 *)(puVar13 + 3) = 0;
          param_1[0xc] = puVar13;
          puVar12[0xc] = puVar13;
          func_0x00010345e360(puVar12,*param_2);
          bVar26 = bVar26 - 1;
          uVar10 = (int)uVar17 + 1U & 7;
          uVar17 = (ulong)uVar10;
          local_100._0_2_ = CONCAT11((char)uVar10,(undefined1)local_100);
          local_100 = CONCAT62(CONCAT51(local_100._3_5_,bVar26),(undefined2)local_100);
        }
        lVar14 = *param_2;
        if (*(char *)(lVar14 + 0xf) == -1) {
          lVar14 = *(long *)(lVar14 + 0x10);
        }
        iVar11 = func_0x00010345e33c(lVar14);
        if (iVar11 != 0) goto LAB_101b51b34;
        bVar15 = *(byte *)((long)param_1 + 0x74);
      } while (7 < bVar26);
    } while( true );
  }
  func_0x000101b51cc8(param_1);
LAB_101b51b34:
  plVar22 = (long *)param_1[0xc];
  uVar21 = param_1[0xf];
  (**(code **)*param_1)(param_1);
  plVar23 = plVar22 + 1;
  LOAcquire();
  iVar11 = (int)*plVar23;
  *(int *)plVar23 = iVar11 + -1;
  LORelease();
  do {
    if (1 < iVar11) {
LAB_101b51b90:
      func_0x00010345e2dc(uVar21,param_1,0x80,param_2);
      if (*_DAT_103fed798 != local_70) {
        func_0x0001034627dc();
        func_0x00010345b87c();
                    /* WARNING: Bad instruction - Truncating control flow here */
        halt_baddata();
      }
      return 0;
    }
    plVar23 = (long *)*plVar22;
    if (plVar23 == (long *)0x0) {
      LOAcquire();
      lVar14 = plVar22[3];
      plVar22[3] = lVar14 + -1;
      LORelease();
      if (lVar14 == 1) {
        func_0x00010345e300(plVar22 + 2);
      }
      goto LAB_101b51b90;
    }
    func_0x00010345e2dc(plVar22[2],plVar22,0x20,param_2);
    LOAcquire();
    iVar11 = *(int *)(plVar23 + 1);
    *(int *)(plVar23 + 1) = iVar11 + -1;
    LORelease();
    plVar22 = plVar23;
  } while( true );
}

