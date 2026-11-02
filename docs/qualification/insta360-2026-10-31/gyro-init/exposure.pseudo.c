
/* WARNING: Type propagation algorithm not settling */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

undefined1  [16] entry(long *param_1,int *param_2)

{
  int *piVar1;
  char cVar2;
  undefined8 ******ppppppuVar3;
  undefined8 uVar4;
  undefined8 uVar5;
  code *pcVar6;
  int iVar7;
  int iVar8;
  undefined4 uVar9;
  long lVar10;
  long lVar11;
  ulong uVar12;
  undefined8 uVar13;
  ulong uVar14;
  long *plVar15;
  double *******pppppppdVar16;
  ulong uVar17;
  ulong uVar18;
  undefined8 *******pppppppuVar19;
  long *plVar20;
  ulong uVar21;
  int iVar22;
  double dVar23;
  undefined1 auVar24 [16];
  double ******ppppppdVar25;
  undefined1 auStack_300 [272];
  long local_1f0;
  undefined8 uStack_1e8;
  undefined8 *local_1e0;
  ulong local_1d8;
  ulong uStack_1d0;
  undefined8 *******local_1c8;
  ulong local_1c0;
  undefined8 uStack_1b8;
  double *******local_1b0;
  undefined6 uStack_1a8;
  undefined2 uStack_1a2;
  undefined4 uStack_1a0;
  undefined1 uStack_19c;
  undefined1 uStack_19b;
  undefined1 uStack_19a;
  byte bStack_199;
  undefined8 *local_190;
  long local_188;
  long lStack_180;
  
  lVar10 = (**(code **)(*param_1 + 8))();
  if (lVar10 == 0) {
    bStack_199 = 0x14;
    uStack_1a0 = 0x2930202c;
    uStack_1a8 = (undefined6)uRam0000000103d68987;
    uStack_1a2 = (undefined2)((ulong)uRam0000000103d68987 >> 0x30);
    local_1b0 = _DAT_103d6897f;
    uStack_19c = 0;
    lVar10 = 0x103d68994;
    lVar11 = func_0x000103463bb0(0x103d68994,0x2f);
    if (lVar11 != 0) {
      lVar10 = lVar11 + 1;
    }
    uVar12 = func_0x000103463b5c(lVar10);
    if (0x7ffffffffffffff7 < uVar12) {
      func_0x000100016eb8(&local_1c8);
      goto LAB_101865d8c;
    }
    if (uVar12 < 0x17) {
      uStack_1b8 = CONCAT17((char)uVar12,(undefined7)uStack_1b8);
      pppppppuVar19 = &local_1c8;
      if (uVar12 != 0) goto LAB_101864fb4;
    }
    else {
      uVar14 = (uVar12 & 0xfffffffffffffff8) + 8;
      if ((uVar12 | 7) != 0x17) {
        uVar14 = uVar12 | 7;
      }
      pppppppuVar19 = (undefined8 *******)func_0x000103462620(uVar14 + 1);
      uStack_1b8 = uVar14 + 1 | 0x8000000000000000;
      local_1c8 = pppppppuVar19;
      local_1c0 = uVar12;
LAB_101864fb4:
      func_0x00010346343c(pppppppuVar19,lVar10,uVar12);
    }
    *(undefined1 *)((long)pppppppuVar19 + uVar12) = 0;
    local_1e0 = (undefined8 *)func_0x000103462620(0x20);
    ppppppuVar3 = _DAT_103d68a10;
    uStack_1d0 = uRam00000001037bed48;
    local_1d8 = _DAT_1037bed40;
    local_1e0[1] = CONCAT35(_DAT_103d68a1d,uRam0000000103d68a18);
    *local_1e0 = ppppppuVar3;
    uVar13 = CONCAT53(uRam0000000103d68a20,_DAT_103d68a1d);
    *(undefined8 *)((long)local_1e0 + 0x15) = uRam0000000103d68a25;
    *(undefined8 *)((long)local_1e0 + 0xd) = uVar13;
    *(undefined1 *)((long)local_1e0 + 0x1d) = 0;
    func_0x00010002bccc(&local_190,&local_1b0,&local_1c8,&local_1e0,0x49);
    func_0x000100016bb0(&local_190,0x103ceb1e6,2);
    uVar13 = (**(code **)(*param_1 + 8))(param_1);
    func_0x000103462008(&local_190,uVar13);
    func_0x000100016bb0(&local_190,0x103cce3bc,3);
    func_0x000103461fe4(&local_190,0);
    func_0x000100016bb0(&local_190,0x103cd5641,1);
    func_0x00010002bfd0(&local_190);
    if ((long)uStack_1d0 < 0) {
      func_0x000103462518(local_1e0);
    }
    if ((long)uStack_1b8 < 0) {
      func_0x000103462518(local_1c8);
    }
    if (-1 < (char)bStack_199) goto LAB_101865098;
    func_0x000103462518(local_1b0);
    cVar2 = (char)param_1[2];
  }
  else {
LAB_101865098:
    cVar2 = (char)param_1[2];
  }
  if (cVar2 == '\0') {
    uStack_1a0 = (undefined4)uRam0000000103d68a3e;
    uStack_19c = (undefined1)((uint6)uRam0000000103d68a3e >> 0x20);
    uStack_19b = (undefined1)((uint6)uRam0000000103d68a3e >> 0x28);
    uStack_19a = 0;
    bStack_199 = 0x16;
    lVar10 = 0x103d68994;
    local_1b0 = _DAT_103d68a2e;
    uStack_1a8 = uRam0000000103d68a36;
    uStack_1a2 = _DAT_103d68a3c;
    lVar11 = func_0x000103463bb0(0x103d68994,0x2f);
    if (lVar11 != 0) {
      lVar10 = lVar11 + 1;
    }
    uVar12 = func_0x000103463b5c(lVar10);
    if (0x7ffffffffffffff7 < uVar12) {
      func_0x000100016eb8(&local_1c8);
      goto LAB_101865d8c;
    }
    if (uVar12 < 0x17) {
      uStack_1b8 = CONCAT17((char)uVar12,(undefined7)uStack_1b8);
      pppppppuVar19 = &local_1c8;
      if (uVar12 != 0) goto LAB_101865138;
    }
    else {
      uVar14 = (uVar12 & 0xfffffffffffffff8) + 8;
      if ((uVar12 | 7) != 0x17) {
        uVar14 = uVar12 | 7;
      }
      pppppppuVar19 = (undefined8 *******)func_0x000103462620(uVar14 + 1);
      uStack_1b8 = uVar14 + 1 | 0x8000000000000000;
      local_1c8 = pppppppuVar19;
      local_1c0 = uVar12;
LAB_101865138:
      func_0x00010346343c(pppppppuVar19,lVar10,uVar12);
    }
    *(undefined1 *)((long)pppppppuVar19 + uVar12) = 0;
    local_1e0 = (undefined8 *)func_0x000103462620(0x20);
    ppppppuVar3 = _DAT_103d68a10;
    uStack_1d0 = uRam00000001037bed48;
    local_1d8 = _DAT_1037bed40;
    local_1e0[1] = CONCAT35(_DAT_103d68a1d,uRam0000000103d68a18);
    *local_1e0 = ppppppuVar3;
    uVar13 = CONCAT53(uRam0000000103d68a20,_DAT_103d68a1d);
    *(undefined8 *)((long)local_1e0 + 0x15) = uRam0000000103d68a25;
    *(undefined8 *)((long)local_1e0 + 0xd) = uVar13;
    *(undefined1 *)((long)local_1e0 + 0x1d) = 0;
    func_0x00010002bccc(&local_190,&local_1b0,&local_1c8,&local_1e0,0x4a);
    func_0x000100016bb0(&local_190,0x103d68a45,0x47);
    func_0x00010002bfd0(&local_190);
    if ((long)uStack_1d0 < 0) {
      func_0x000103462518(local_1e0);
    }
    if ((long)uStack_1b8 < 0) {
      func_0x000103462518(local_1c8);
    }
    if ((char)bStack_199 < '\0') {
      func_0x000103462518(local_1b0);
    }
  }
  if (*(char *)((long)param_1 + 0xb9) != '\0') {
LAB_101865234:
    dVar23 = (double)func_0x000101ad5dec(param_2 + 2);
    if ((char)param_1[2] != '\0') {
      dVar23 = (dVar23 / 1000.0) * (double)param_1[1];
      goto LAB_10186525c;
    }
    func_0x0001001b88bc();
LAB_101865d44:
    func_0x000100016eb8(&local_1b0);
LAB_101865d8c:
                    /* WARNING: Does not return */
    pcVar6 = (code *)SoftwareBreakpoint(1,0x101865d90);
    (*pcVar6)();
  }
  lVar10 = param_1[0x15] - param_1[0x14];
  if ((lVar10 == 0) ||
     (uVar12 = (lVar10 >> 3) * -0x5555555555555555, iVar7 = *param_2,
     uVar12 < (ulong)(long)iVar7 || uVar12 - (long)iVar7 == 0)) goto LAB_101865234;
  plVar20 = (long *)(param_1[0x14] + (long)iVar7 * 0x18);
  local_1b0 = (double *******)0x0;
  uStack_1a8 = 0;
  uStack_1a2 = 0;
  uStack_1a0 = 0;
  uStack_19c = 0;
  uStack_19b = 0;
  uStack_19a = 0;
  bStack_199 = 0;
  lVar10 = *plVar20;
  lVar11 = plVar20[1] - lVar10;
  if (lVar11 == 0) {
    lVar10 = 0x103d68994;
    lVar11 = func_0x000103463bb0(0x103d68994,0x2f);
    if (lVar11 != 0) {
      lVar10 = lVar11 + 1;
    }
    uVar12 = func_0x000103463b5c(lVar10);
    if (0x7ffffffffffffff7 < uVar12) {
      func_0x000100016eb8(&local_1c8);
      goto LAB_101865d8c;
    }
    if (uVar12 < 0x17) {
      uStack_1b8 = CONCAT17((char)uVar12,(undefined7)uStack_1b8);
      pppppppuVar19 = &local_1c8;
      if (uVar12 != 0) goto LAB_10186593c;
    }
    else {
      uVar14 = (uVar12 & 0xfffffffffffffff8) + 8;
      if ((uVar12 | 7) != 0x17) {
        uVar14 = uVar12 | 7;
      }
      pppppppuVar19 = (undefined8 *******)func_0x000103462620(uVar14 + 1);
      uStack_1b8 = uVar14 + 1 | 0x8000000000000000;
      local_1c8 = pppppppuVar19;
      local_1c0 = uVar12;
LAB_10186593c:
      func_0x00010346343c(pppppppuVar19,lVar10,uVar12);
    }
    *(undefined1 *)((long)pppppppuVar19 + uVar12) = 0;
    local_1e0 = (undefined8 *)func_0x000103462620(0x20);
    ppppppuVar3 = _DAT_103d68a10;
    uStack_1d0 = uRam00000001037bed48;
    local_1d8 = _DAT_1037bed40;
    local_1e0[1] = CONCAT35(_DAT_103d68a1d,uRam0000000103d68a18);
    *local_1e0 = ppppppuVar3;
    uVar13 = CONCAT53(uRam0000000103d68a20,_DAT_103d68a1d);
    *(undefined8 *)((long)local_1e0 + 0x15) = uRam0000000103d68a25;
    *(undefined8 *)((long)local_1e0 + 0xd) = uVar13;
    *(undefined1 *)((long)local_1e0 + 0x1d) = 0;
    func_0x00010002b8a0(&local_190,&local_1c8,&local_1e0,0x5a,3);
    func_0x000100016bb0(&local_190,0x103d68a8d,0x21);
    func_0x000103461fe4(&local_190,*param_2);
    func_0x00010002bfd0(&local_190);
    if ((long)uStack_1d0 < 0) {
      func_0x000103462518(local_1e0);
    }
    if ((long)uStack_1b8 < 0) {
      func_0x000103462518(local_1c8);
    }
    ppppppdVar25 = (double ******)func_0x000101ad5dec(param_2 + 2);
    if ((char)param_1[2] == '\0') {
      func_0x0001001b88bc();
      goto LAB_101865d8c;
    }
LAB_1018659f4:
    dVar23 = ((double)ppppppdVar25 / 1000.0) * (double)param_1[1];
  }
  else {
    if (lVar11 < 0) {
      func_0x000100023e9c(&local_1b0);
      goto LAB_101865d8c;
    }
    local_1b0 = (double *******)func_0x000103462620(lVar11);
    uStack_1a8 = SUB86(local_1b0,0);
    uStack_1a2 = (undefined2)((ulong)local_1b0 >> 0x30);
    pppppppdVar16 = local_1b0 + (lVar11 >> 3);
    uStack_1a0 = SUB84(pppppppdVar16,0);
    uStack_19c = (undefined1)((ulong)pppppppdVar16 >> 0x20);
    uStack_19b = (undefined1)((ulong)pppppppdVar16 >> 0x28);
    uStack_19a = (undefined1)((ulong)pppppppdVar16 >> 0x30);
    bStack_199 = (byte)((ulong)pppppppdVar16 >> 0x38);
    func_0x000103463430(local_1b0,lVar10,lVar11);
    uStack_1a8 = SUB86(pppppppdVar16,0);
    uStack_1a2 = (undefined2)((ulong)pppppppdVar16 >> 0x30);
    piVar1 = param_2 + 2;
    func_0x000101ad5dec(piVar1);
    uVar12 = func_0x00010346034c(&local_1b0,0,
                                 (ulong)(CONCAT26(uStack_1a2,uStack_1a8) - (long)local_1b0) >> 3);
    iVar7 = (int)uVar12;
    if ((iVar7 < 0) ||
       ((ulong)(CONCAT26(uStack_1a2,uStack_1a8) - (long)local_1b0 >> 3) <= (uVar12 & 0xffffffff))) {
      ppppppdVar25 = (double ******)func_0x000101ad5dec(piVar1);
      if ((char)param_1[2] == '\0') {
        func_0x0001001b88bc();
        goto LAB_101865d8c;
      }
      goto LAB_1018659f4;
    }
    ppppppdVar25 = local_1b0[uVar12 & 0xffffffff];
    dVar23 = (double)func_0x000101ad5dec(piVar1);
    uVar14 = uVar12;
    if ((1.0 <= ABS((double)ppppppdVar25 - dVar23)) &&
       ((uVar14 = (ulong)(iVar7 + 1),
        (ulong)(CONCAT26(uStack_1a2,uStack_1a8) - (long)local_1b0 >> 3) <= uVar14 ||
        (ppppppdVar25 = local_1b0[uVar14], dVar23 = (double)func_0x000101ad5dec(piVar1),
        1.0 <= ABS((double)ppppppdVar25 - dVar23))))) {
      if (iVar7 != 0) {
        ppppppdVar25 = local_1b0[iVar7 - 1];
        dVar23 = (double)func_0x000101ad5dec(piVar1);
        uVar14 = (ulong)(iVar7 - 1);
        if (ABS((double)ppppppdVar25 - dVar23) < 1.0) goto LAB_10186549c;
      }
      lVar10 = 0x103d68994;
      lVar11 = func_0x000103463bb0(0x103d68994,0x2f);
      if (lVar11 != 0) {
        lVar10 = lVar11 + 1;
      }
      func_0x000101e51504(&local_1c8,lVar10);
      func_0x000101e51504(&local_1e0,&DAT_103d68a10);
      func_0x00010002b8a0(&local_190,&local_1c8,&local_1e0,0x6f,2);
      func_0x000100016bb0(&local_190,0x103d68aaf,0x38);
      func_0x00010002bfd0(&local_190);
      if ((long)uStack_1d0 < 0) {
        func_0x000103462518(local_1e0);
      }
      uVar14 = uVar12;
      if ((long)uStack_1b8 < 0) {
        func_0x000103462518(local_1c8);
      }
    }
LAB_10186549c:
    iVar7 = *param_2;
    uVar12 = (ulong)iVar7;
    iVar8 = (int)uVar14;
    if (*(char *)((long)param_1 + 0xba) == '\0') {
      if (((char)param_1[0x17] == '\0') || ((ulong)(param_1[10] - param_1[9] >> 2) <= uVar12)) {
        if ((char)param_1[2] == '\0') {
          func_0x0001001b88bc();
          goto LAB_101865d8c;
        }
        ppppppdVar25 = local_1b0[uVar14 & 0xffffffff];
        goto LAB_1018659f4;
      }
      iVar7 = *(int *)(param_1[9] + uVar12 * 4);
      uVar13 = (**(code **)(*(long *)param_1[6] + 0x10))((long *)param_1[6],&local_190,(long)iVar7);
      ppppppdVar25 = local_1b0[uVar14 & 0xffffffff];
      dVar23 = (double)param_1[8];
      local_1c8 = (undefined8 *******)param_1[6];
      local_1c0 = param_1[7];
      lVar10 = 0;
      if (local_1c0 != 0) {
        lVar10 = *(long *)(local_1c0 + 8);
        *(long *)(local_1c0 + 8) = lVar10 + 1;
      }
      iVar8 = func_0x00010186604c(lVar10,(double)(long)local_190 + dVar23 * (double)ppppppdVar25,
                                  uVar13,&local_1c8);
      func_0x0001000186e4(&local_1c8);
      dVar23 = (double)(iVar8 - iVar7);
    }
    else if ((((uVar12 < (ulong)(param_1[10] - param_1[9] >> 2)) &&
              ((long *)param_1[6] != (long *)0x0)) &&
             (lVar10 = (**(code **)(*(long *)param_1[6] + 0x58))(), lVar10 != 0)) &&
            (local_1b0 != (double *******)CONCAT26(uStack_1a2,uStack_1a8))) {
      func_0x000103462230(param_1 + 0xc);
      plVar20 = param_1 + 0x18;
      lVar10 = *plVar20;
      if ((int)((ulong)(param_1[0x19] - lVar10) >> 2) <= iVar7) {
        local_190 = (undefined8 *)CONCAT44(local_190._4_4_,0xffffffff);
        func_0x000101866020(plVar20,uVar12 + 1,&local_190);
        lVar10 = *plVar20;
      }
      if (*(int *)(lVar10 + uVar12 * 4) < 0) {
        iVar22 = *(int *)(param_1[9] + uVar12 * 4);
        lVar10 = (long)iVar22;
        plVar15 = (long *)param_1[6];
        if (((iVar7 == 0) && (-1 < (int)param_1[0x1b])) &&
           ((plVar15 != (long *)0x0 && (uVar14 = (int)param_1[0x1b] + lVar10, -1 < (long)uVar14))))
        {
          uVar17 = (**(code **)(*plVar15 + 0x58))();
          if (uVar17 <= uVar14) {
            iVar22 = *(int *)param_1[9];
            plVar15 = (long *)param_1[6];
            lVar10 = (long)iVar22;
            goto LAB_101865ad0;
          }
          *(int *)*plVar20 = (int)uVar14;
          lVar10 = 0x103d68994;
          lVar11 = func_0x000103463bb0(0x103d68994,0x2f);
          if (lVar11 != 0) {
            lVar10 = lVar11 + 1;
          }
          func_0x000101e51504(&local_1c8,lVar10);
          func_0x000101e51504(&local_1e0,&DAT_103d68a10);
          func_0x00010002b8a0(&local_190,&local_1c8,&local_1e0,0x8f,1);
          func_0x000100016bb0(&local_190,0x103d68ae8,0x41);
          func_0x000103461fe4(&local_190,(int)param_1[0x1b]);
          func_0x000100016bb0(&local_190,0x103d68b2a,0x12);
          func_0x000103461fe4(&local_190,*(undefined4 *)param_1[9]);
          func_0x000100016bb0(&local_190,0x103d68b3d,0xf);
          func_0x000103461fe4(&local_190,*(undefined4 *)*plVar20);
        }
        else {
LAB_101865ad0:
          local_1f0 = 0;
          uStack_1e8 = 0;
          uVar13 = (**(code **)(*plVar15 + 0x10))(plVar15,&local_1f0,lVar10);
          if ((int)uVar13 == 0) {
            lVar10 = 0x103d68994;
            lVar11 = func_0x000103463bb0(0x103d68994,0x2f);
            if (lVar11 != 0) {
              lVar10 = lVar11 + 1;
            }
            func_0x000101e51504(&local_1c8,lVar10);
            func_0x000101e51504(&local_1e0,&DAT_103d68a10);
            func_0x00010002b8a0(&local_190,&local_1c8,&local_1e0,0xa1,2);
            func_0x000100016bb0(&local_190,0x103d68bb5,0x39);
            func_0x000103461fe4(&local_190,uVar12);
            func_0x000100016bb0(&local_190,0x103d68b73,0xc);
            func_0x000103461fe4(&local_190,iVar22);
            func_0x000100016bb0(&local_190,0x103d68bef,0xe);
          }
          else {
            dVar23 = (double)local_1f0 + (double)param_1[8] * (double)*local_1b0;
            local_190 = (undefined8 *)param_1[6];
            local_188 = param_1[7];
            lVar10 = 0;
            if (local_188 != 0) {
              lVar10 = *(long *)(local_188 + 8);
              *(long *)(local_188 + 8) = lVar10 + 1;
            }
            uVar9 = func_0x00010186604c(lVar10,dVar23,uVar13,&local_190);
            *(undefined4 *)(*plVar20 + uVar12 * 4) = uVar9;
            func_0x0001000186e4(&local_190);
            lVar10 = 0x103d68994;
            lVar11 = func_0x000103463bb0(0x103d68994,0x2f);
            if (lVar11 != 0) {
              lVar10 = lVar11 + 1;
            }
            func_0x000101e51504(&local_1c8,lVar10);
            func_0x000101e51504(&local_1e0,&DAT_103d68a10);
            func_0x00010002b8a0(&local_190,&local_1c8,&local_1e0,0x99,1);
            func_0x000100016bb0(&local_190,0x103d68b4d,0x25);
            func_0x000103461fe4(&local_190,uVar12);
            func_0x000100016bb0(&local_190,0x103d68b73,0xc);
            func_0x000103461fe4(&local_190,iVar22);
            func_0x000100016bb0(&local_190,0x103d68b80,0xd);
            func_0x00010346202c(&local_190,local_1f0);
            func_0x000100016bb0(&local_190,0x103d68b8e,0xe);
            func_0x000103461fcc(*local_1b0,&local_190);
            func_0x000100016bb0(&local_190,0x103d68b9d,10);
            func_0x00010346202c(&local_190,(long)dVar23);
            func_0x000100016bb0(&local_190,0x103d68ba8,0xc);
            func_0x000103461fe4(&local_190,*(undefined4 *)(*plVar20 + uVar12 * 4));
          }
        }
        func_0x00010002bfd0(&local_190);
        if ((long)uStack_1d0 < 0) {
          func_0x000103462518(local_1e0);
        }
        if ((long)uStack_1b8 < 0) {
          func_0x000103462518(local_1c8);
        }
      }
      iVar7 = *(int *)(*plVar20 + uVar12 * 4);
      if (-1 < iVar7) {
        iVar8 = (iVar7 + iVar8) - *(int *)(param_1[9] + uVar12 * 4);
      }
      dVar23 = (double)iVar8;
      func_0x00010346223c(param_1 + 0xc);
    }
    else {
      dVar23 = (double)iVar8;
    }
  }
  if (local_1b0 != (double *******)0x0) {
    uStack_1a8 = SUB86(local_1b0,0);
    uStack_1a2 = (undefined2)((ulong)local_1b0 >> 0x30);
    func_0x000103462518();
  }
LAB_10186525c:
  uVar12 = (ulong)*param_2;
  lVar10 = param_1[9];
  if ((ulong)(param_1[10] - lVar10 >> 2) <= uVar12) {
    local_190 = (undefined8 *)func_0x000103462620(0x30);
    uVar5 = _DAT_103d68c0e;
    uVar4 = _DAT_103d68bfe;
    lStack_180 = lRam0000000103870518;
    local_188 = _DAT_103870510;
    uVar13 = CONCAT44(_DAT_103d68c1a,uRam0000000103d68c16);
    local_190[1] = uRam0000000103d68c06;
    *local_190 = uVar4;
    local_190[3] = uVar13;
    local_190[2] = uVar5;
    uVar13 = CONCAT44(uRam0000000103d68c1e,_DAT_103d68c1a);
    *(undefined8 *)((long)local_190 + 0x24) = uRam0000000103d68c22;
    *(undefined8 *)((long)local_190 + 0x1c) = uVar13;
    *(undefined1 *)((long)local_190 + 0x2c) = 0;
    lVar10 = 0x103d68994;
    lVar11 = func_0x000103463bb0(0x103d68994,0x2f);
    if (lVar11 != 0) {
      lVar10 = lVar11 + 1;
    }
    uVar14 = func_0x000103463b5c(lVar10);
    if (0x7ffffffffffffff7 < uVar14) goto LAB_101865d44;
    if (uVar14 < 0x17) {
      bStack_199 = (byte)uVar14;
      pppppppdVar16 = (double *******)&local_1b0;
      if (uVar14 != 0) goto LAB_101865650;
    }
    else {
      uVar17 = (uVar14 & 0xfffffffffffffff8) + 8;
      if ((uVar14 | 7) != 0x17) {
        uVar17 = uVar14 | 7;
      }
      lVar11 = uVar17 + 1;
      pppppppdVar16 = (double *******)func_0x000103462620(lVar11);
      bStack_199 = (byte)((ulong)lVar11 >> 0x38) | 0x80;
      uStack_1a8 = (undefined6)uVar14;
      uStack_1a2 = (undefined2)(uVar14 >> 0x30);
      uStack_1a0 = (undefined4)lVar11;
      uStack_19c = (undefined1)((ulong)lVar11 >> 0x20);
      uStack_19b = (undefined1)((ulong)lVar11 >> 0x28);
      uStack_19a = (undefined1)((ulong)lVar11 >> 0x30);
      local_1b0 = pppppppdVar16;
LAB_101865650:
      func_0x00010346343c(pppppppdVar16,lVar10,uVar14);
    }
    *(undefined1 *)((long)pppppppdVar16 + uVar14) = 0;
    local_1c8 = (undefined8 *******)func_0x000103462620(0x20);
    ppppppuVar3 = _DAT_103d68a10;
    uStack_1b8 = uRam00000001037bed48;
    local_1c0 = _DAT_1037bed40;
    local_1c8[1] = (undefined8 ******)CONCAT35(_DAT_103d68a1d,uRam0000000103d68a18);
    *local_1c8 = ppppppuVar3;
    uVar13 = CONCAT53(uRam0000000103d68a20,_DAT_103d68a1d);
    *(undefined8 *)((long)local_1c8 + 0x15) = uRam0000000103d68a25;
    *(undefined8 *)((long)local_1c8 + 0xd) = uVar13;
    *(undefined1 *)((long)local_1c8 + 0x1d) = 0;
    func_0x00010002bccc(auStack_300,&local_190,&local_1b0,&local_1c8,0xd1);
    func_0x00010002bfd0(auStack_300);
    if ((long)uStack_1b8 < 0) {
      func_0x000103462518(local_1c8);
    }
    if ((char)bStack_199 < '\0') {
      func_0x000103462518(local_1b0);
    }
    if (lStack_180 < 0) {
      func_0x000103462518(local_190);
    }
    lVar10 = param_1[9];
  }
  uVar14 = (long)*(int *)(lVar10 + uVar12 * 4) + (long)(int)dVar23;
  uVar17 = (**(code **)(*(long *)param_1[6] + 0x58))();
  if (uVar17 <= uVar14) {
    iVar7 = (**(code **)(*(long *)param_1[6] + 0x58))();
    uVar14 = (ulong)(iVar7 - 1);
  }
  if (0.009999999776482582 <= ABS(dVar23 - (double)(int)dVar23)) {
    uVar14 = (long)*(int *)(param_1[9] + uVar12 * 4) + (long)(int)dVar23;
    uVar17 = (**(code **)(*(long *)param_1[6] + 0x58))();
    if (uVar17 <= uVar14) {
      iVar7 = (**(code **)(*(long *)param_1[6] + 0x58))();
      uVar14 = (ulong)(iVar7 - 1);
    }
    uVar17 = (long)*(int *)(param_1[9] + uVar12 * 4) + (long)(int)dVar23;
    uVar18 = (**(code **)(*(long *)param_1[6] + 0x58))();
    uVar21 = uVar17;
    if (uVar18 <= uVar17) {
      iVar7 = (**(code **)(*(long *)param_1[6] + 0x58))();
      uVar17 = (ulong)(int)(iVar7 - 1U);
      uVar21 = (ulong)(iVar7 - 1U);
    }
    iVar8 = (int)uVar14;
    (**(code **)(*(long *)param_1[6] + 0x10))((long *)param_1[6],&local_190,(long)iVar8);
    (**(code **)(*(long *)param_1[6] + 0x10))((long *)param_1[6],&local_1b0,uVar17);
    iVar7 = (int)uVar21 - iVar8;
    if (iVar7 != 0) {
      dVar23 = (double)(long)local_190 +
               (double)((long)local_1b0 - (long)local_190) *
               (((dVar23 + (double)(long)*(int *)(param_1[9] + uVar12 * 4)) - (double)iVar8) /
               (double)iVar7);
      goto LAB_101865854;
    }
  }
  else {
    (**(code **)(*(long *)param_1[6] + 0x10))((long *)param_1[6],&local_190,(long)(int)uVar14);
  }
  dVar23 = (double)(long)local_190;
LAB_101865854:
  auVar24._0_8_ = dVar23 / (double)param_1[8];
  auVar24._8_8_ = 0;
  return auVar24;
}

