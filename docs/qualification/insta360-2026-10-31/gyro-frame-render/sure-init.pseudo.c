
/* WARNING: Type propagation algorithm not settling */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(long param_1,undefined8 param_2,undefined8 param_3,undefined8 param_4)

{
  long *plVar1;
  code *pcVar2;
  undefined8 *****pppppuVar3;
  int iVar4;
  int iVar5;
  long lVar6;
  undefined8 *******pppppppuVar7;
  undefined8 *******pppppppuVar8;
  undefined8 uVar9;
  ulong uVar10;
  undefined8 *puVar11;
  undefined8 *puVar12;
  long lVar13;
  long lVar14;
  uint uVar15;
  undefined4 uVar16;
  int iVar17;
  long *plVar18;
  int iVar19;
  undefined8 ******ppppppuVar20;
  long lVar21;
  uint uVar22;
  double dVar23;
  undefined8 uVar24;
  undefined8 uVar25;
  int local_208;
  int iStack_204;
  undefined4 local_200;
  undefined4 uStack_1fc;
  undefined4 local_1f8;
  undefined4 uStack_1f4;
  undefined8 *******local_1f0;
  long *plStack_1e8;
  undefined2 local_1e0;
  char local_1d9;
  int local_1d8;
  int iStack_1d4;
  undefined4 local_1d0;
  undefined4 uStack_1cc;
  undefined4 local_1c8;
  undefined4 uStack_1c4;
  long local_1c0;
  long *local_1b8;
  undefined8 *******local_1b0;
  long *plStack_1a8;
  undefined8 local_1a0;
  undefined8 *******local_190;
  undefined8 *******local_188;
  undefined8 uStack_180;
  undefined8 *local_178;
  undefined8 *local_170;
  undefined8 *local_168;
  
  lVar21 = *(long *)(param_1 + 0x10);
  if (lVar21 == 0) goto LAB_1010806ac;
  lVar6 = func_0x00010346274c(lVar21,0x1040003f8,0x1040a4750,0);
  if (lVar6 == 0) {
    local_1c0 = func_0x00010346274c(lVar21,0x1040003f8,0x1040a2d28,0);
    iVar19 = (int)param_3;
    iVar17 = (int)param_4;
    if (local_1c0 == 0) {
      local_1c0 = 0;
      local_1b8 = (long *)0x0;
      if (*(long *)(param_1 + 0x20) != 0) {
        if ((*(int *)(param_1 + 0x84) == iVar19) && (*(int *)(param_1 + 0x88) == iVar17))
        goto LAB_1010812e8;
        lVar21 = 0x103d1e53f;
        lVar6 = func_0x000103463bb0(0x103d1e53f,0x2f);
        if (lVar6 != 0) {
          lVar21 = lVar6 + 1;
        }
        pppppppuVar7 = (undefined8 *******)func_0x000103463b5c(lVar21);
        if ((undefined8 *******)0x7ffffffffffffff7 < pppppppuVar7) {
          func_0x000100016eb8(&local_190);
          goto LAB_101081378;
        }
        if (pppppppuVar7 < (undefined8 *******)0x17) {
          uStack_180 = CONCAT17((char)pppppppuVar7,(undefined7)uStack_180);
          pppppppuVar8 = &local_190;
          if (pppppppuVar7 != (undefined8 *******)0x0) goto LAB_101080a90;
        }
        else {
          uVar10 = ((ulong)pppppppuVar7 & 0xfffffffffffffff8) + 8;
          if (((ulong)pppppppuVar7 | 7) != 0x17) {
            uVar10 = (ulong)pppppppuVar7 | 7;
          }
          pppppppuVar8 = (undefined8 *******)func_0x000103462620(uVar10 + 1);
          uStack_180 = uVar10 + 1 | 0x8000000000000000;
          local_190 = pppppppuVar8;
          local_188 = pppppppuVar7;
LAB_101080a90:
          func_0x00010346343c(pppppppuVar8,lVar21,pppppppuVar7);
        }
        *(undefined1 *)((long)pppppppuVar8 + (long)pppppppuVar7) = 0;
        local_1a0 = CONCAT17(0x11,(undefined7)local_1a0);
        plStack_1a8 = plRam0000000103d1e5d8;
        local_1b0 = _DAT_103d1e5d0;
        local_1a0 = CONCAT62(local_1a0._2_6_,0x65);
        func_0x00010002b8a0(&local_178,&local_190,&local_1b0,0x72,1);
        func_0x000100016bb0(&local_178,0x103d1e632,0x1c);
        func_0x000103461fe4(&local_178,*(undefined4 *)(param_1 + 0x84));
        func_0x000100016bb0(&local_178,0x103cd51a7,1);
        func_0x000103461fe4(&local_178,*(undefined4 *)(param_1 + 0x88));
        func_0x000100016bb0(&local_178,0x103cce3d1,2);
        func_0x000103461fe4(&local_178,param_3);
        func_0x000100016bb0(&local_178,0x103cd51a7,1);
        func_0x000103461fe4(&local_178,param_4);
        goto LAB_101080b5c;
      }
    }
    else {
      local_1b8 = *(long **)(param_1 + 0x18);
      if (local_1b8 != (long *)0x0) {
        local_1b8[1] = local_1b8[1] + 1;
      }
      if (*(long *)(param_1 + 0x20) == 0) goto LAB_101080bd0;
      uVar9 = func_0x000101833558();
      if ((*(int *)(param_1 + 0xa8) == (int)uVar9) &&
         (*(int *)(param_1 + 0xac) == (int)((ulong)uVar9 >> 0x20))) goto LAB_1010812e8;
      lVar21 = 0x103d1e53f;
      lVar6 = func_0x000103463bb0(0x103d1e53f,0x2f);
      if (lVar6 != 0) {
        lVar21 = lVar6 + 1;
      }
      pppppppuVar7 = (undefined8 *******)func_0x000103463b5c(lVar21);
      if ((undefined8 *******)0x7ffffffffffffff7 < pppppppuVar7) {
        func_0x000100016eb8(&local_190);
        goto LAB_101081378;
      }
      if (pppppppuVar7 < (undefined8 *******)0x17) {
        uStack_180 = CONCAT17((char)pppppppuVar7,(undefined7)uStack_180);
        pppppppuVar8 = &local_190;
        if (pppppppuVar7 != (undefined8 *******)0x0) goto LAB_10108097c;
      }
      else {
        uVar10 = ((ulong)pppppppuVar7 & 0xfffffffffffffff8) + 8;
        if (((ulong)pppppppuVar7 | 7) != 0x17) {
          uVar10 = (ulong)pppppppuVar7 | 7;
        }
        pppppppuVar8 = (undefined8 *******)func_0x000103462620(uVar10 + 1);
        uStack_180 = uVar10 + 1 | 0x8000000000000000;
        local_190 = pppppppuVar8;
        local_188 = pppppppuVar7;
LAB_10108097c:
        func_0x00010346343c(pppppppuVar8,lVar21,pppppppuVar7);
      }
      *(undefined1 *)((long)pppppppuVar8 + (long)pppppppuVar7) = 0;
      local_1a0 = CONCAT17(0x11,(undefined7)local_1a0);
      plStack_1a8 = plRam0000000103d1e5d8;
      local_1b0 = _DAT_103d1e5d0;
      local_1a0 = CONCAT62(local_1a0._2_6_,0x65);
      func_0x00010002b8a0(&local_178,&local_190,&local_1b0,0x6a,1);
      func_0x000100016bb0(&local_178,0x103d1e5f8,0x25);
      lVar21 = local_1c0;
      uVar9 = func_0x000101833558(local_1c0);
      func_0x000103461fe4(&local_178,uVar9);
      func_0x000100016bb0(&local_178,0x103cce1df,2);
      uVar10 = func_0x000101833558(lVar21);
      func_0x000103461fe4(&local_178,uVar10 >> 0x20);
      func_0x000100016bb0(&local_178,0x103d1e61e,0x13);
      func_0x000103461fe4(&local_178,*(undefined4 *)(param_1 + 0xa8));
      func_0x000100016bb0(&local_178,0x103cce1df,2);
      func_0x000103461fe4(&local_178,*(undefined4 *)(param_1 + 0xac));
LAB_101080b5c:
      func_0x00010002bfd0(&local_178);
      if ((long)local_1a0 < 0) {
        func_0x000103462518(local_1b0);
        if (-1 < (long)uStack_180) goto LAB_101080b74;
LAB_101080b94:
        func_0x000103462518(local_190);
        plVar18 = *(long **)(param_1 + 0x28);
        *(undefined8 *)(param_1 + 0x20) = 0;
        *(undefined8 *)(param_1 + 0x28) = 0;
      }
      else {
        if ((long)uStack_180 < 0) goto LAB_101080b94;
LAB_101080b74:
        plVar18 = *(long **)(param_1 + 0x28);
        *(undefined8 *)(param_1 + 0x20) = 0;
        *(undefined8 *)(param_1 + 0x28) = 0;
      }
      if (plVar18 != (long *)0x0) {
        LOAcquire();
        lVar21 = plVar18[1];
        plVar18[1] = lVar21 + -1;
        LORelease();
        if (lVar21 == 0) {
          (**(code **)(*plVar18 + 0x10))(plVar18);
          func_0x000103462194(plVar18);
        }
      }
    }
LAB_101080bd0:
    lVar21 = func_0x0001034622d8();
    iVar4 = func_0x0001018b5e9c(param_2);
    puVar11 = (undefined8 *)func_0x000103462620(0x100);
    puVar11[2] = 0;
    *puVar11 = 0x10407b218;
    puVar11[1] = 0;
    puVar11[7] = 0;
    puVar11[6] = 0;
    puVar11[9] = 0;
    puVar11[8] = 0;
    puVar11[0xb] = 0;
    puVar11[10] = 0;
    puVar11[0xc] = 0;
    puVar11[5] = 0;
    puVar11[4] = 0;
    puVar11[0xe] = 0;
    puVar11[0xf] = 0;
    puVar11[0x10] = 0x3ff0000000000000;
    puVar11[0x12] = 0;
    puVar11[0x11] = 0;
    puVar11[0x14] = 0;
    puVar11[0x13] = 0;
    puVar11[0x15] = _DAT_1039111d8;
    puVar11[0x19] = 0;
    puVar11[0x18] = 0;
    puVar11[0x1b] = 0;
    puVar11[0x1a] = 0;
    puVar11[0x17] = 0;
    puVar11[0x16] = 0;
    puVar11[0x1c] = 0;
    puVar11[0x1d] = 0x40278d0a;
    *(undefined1 *)(puVar11 + 0x1e) = 0;
    puVar11[0x19] = 0x3f989374bc6a7efa;
    plVar18 = *(long **)(param_1 + 0x28);
    *(undefined8 **)(param_1 + 0x20) = puVar11 + 4;
    *(undefined8 **)(param_1 + 0x28) = puVar11;
    if (plVar18 != (long *)0x0) {
      LOAcquire();
      lVar6 = plVar18[1];
      plVar18[1] = lVar6 + -1;
      LORelease();
      if (lVar6 == 0) {
        (**(code **)(*plVar18 + 0x10))(plVar18);
        func_0x000103462194(plVar18);
      }
    }
    if (*(char *)(param_1 + 0xa1) == '\0') {
      uVar22 = (uint)*(byte *)(param_1 + 0xa2);
    }
    else {
      uVar22 = 2;
    }
    iVar5 = func_0x0001018b0cd4(param_2);
    if (iVar5 == 6) {
      uVar15 = 0;
    }
    else {
      uVar15 = 0;
      if (*(char *)(param_1 + 0xa3) != '\0') {
        uVar15 = uVar22;
      }
    }
    *(uint *)(*(long *)(param_1 + 0x20) + 0xcc) = uVar15;
    iVar5 = (**(code **)(**(long **)(param_1 + 0x10) + 0x78))();
    if (iVar5 != 0) {
      (**(code **)(**(long **)(param_1 + 0x10) + 0x80))(&local_190);
      uVar10 = (**(code **)(**(long **)(param_1 + 0x10) + 0x90))();
      if ((uVar10 & 1) == 0) {
        local_178 = (undefined8 *)0x0;
        local_170 = (undefined8 *)0x0;
        local_168 = (undefined8 *)0x0;
        puVar12 = (undefined8 *)func_0x000103462620(0x20);
        local_168 = puVar12 + 4;
        *puVar12 = 0;
        puVar12[1] = 0;
        puVar12[2] = 0;
        puVar12[3] = 0x3ff0000000000000;
        local_178 = puVar12;
        local_178 = (undefined8 *)func_0x000103462620(0x40);
        puVar11 = local_178 + 8;
        local_178[4] = 0;
        local_178[5] = 0;
        local_178[6] = 0;
        local_178[7] = 0x3ff0000000000000;
        uVar9 = *puVar12;
        uVar25 = puVar12[3];
        uVar24 = puVar12[2];
        local_178[1] = puVar12[1];
        *local_178 = uVar9;
        local_178[3] = uVar25;
        local_178[2] = uVar24;
        local_170 = puVar11;
        local_168 = puVar11;
        func_0x000103462518(puVar12);
        local_170 = puVar11;
        func_0x000101b577c4(&local_190,&local_178);
        (**(code **)(**(long **)(param_1 + 0x10) + 0x88))(*(long **)(param_1 + 0x10),1);
        if (local_178 != (undefined8 *)0x0) {
          local_170 = local_178;
          func_0x000103462518();
        }
      }
      local_1d0 = *(undefined4 *)(param_1 + 0x8c);
      uStack_1cc = *(undefined4 *)(param_1 + 0x90);
      local_1d8 = iVar19;
      iStack_1d4 = iVar17;
      local_1c8 = local_1d0;
      uStack_1c4 = uStack_1cc;
      func_0x000101b54854(*(undefined8 *)(param_1 + 0x20),param_2,&local_1c8,&local_1d0,&local_1d8,
                          &local_190,1,*(undefined1 *)(param_1 + 0xa4));
      lVar6 = 0x103d1e53f;
      lVar13 = func_0x000103463bb0(0x103d1e53f,0x2f);
      if (lVar13 != 0) {
        lVar6 = lVar13 + 1;
      }
      plVar18 = (long *)func_0x000103463b5c(lVar6);
      if ((long *)0x7ffffffffffffff7 < plVar18) {
        func_0x000100016eb8(&local_1b0);
        goto LAB_101081378;
      }
      if (plVar18 < (long *)0x17) {
        local_1a0 = CONCAT17((char)plVar18,(undefined7)local_1a0);
        pppppppuVar7 = &local_1b0;
        if (plVar18 != (long *)0x0) goto LAB_101080f90;
      }
      else {
        uVar10 = ((ulong)plVar18 & 0xfffffffffffffff8) + 8;
        if (((ulong)plVar18 | 7) != 0x17) {
          uVar10 = (ulong)plVar18 | 7;
        }
        pppppppuVar7 = (undefined8 *******)func_0x000103462620(uVar10 + 1);
        local_1a0 = uVar10 + 1 | 0x8000000000000000;
        local_1b0 = pppppppuVar7;
        plStack_1a8 = plVar18;
LAB_101080f90:
        func_0x00010346343c(pppppppuVar7,lVar6,plVar18);
      }
      *(undefined1 *)((long)pppppppuVar7 + (long)plVar18) = 0;
      local_1d9 = '\x11';
      plStack_1e8 = plRam0000000103d1e5d8;
      local_1f0 = _DAT_103d1e5d0;
      local_1e0 = 0x65;
      func_0x00010002b8a0(&local_178,&local_1b0,&local_1f0,0x93,1);
      func_0x000100016bb0(&local_178,0x103d1b455,0x24);
      func_0x00010002bfd0(&local_178);
      if (local_1d9 < '\0') {
        func_0x000103462518(local_1f0);
      }
      pppppppuVar7 = local_190;
      if ((long)local_1a0 < 0) {
        func_0x000103462518(local_1b0);
        pppppppuVar7 = local_190;
      }
      local_190 = pppppppuVar7;
      pppppppuVar8 = local_188;
      if (pppppppuVar7 != (undefined8 *******)0x0) {
        for (; pppppppuVar8 != pppppppuVar7; pppppppuVar8 = pppppppuVar8 + -2) {
          ppppppuVar20 = pppppppuVar8[-1];
          if (ppppppuVar20 != (undefined8 ******)0x0) {
            LOAcquire();
            pppppuVar3 = ppppppuVar20[1];
            ppppppuVar20[1] = (undefined8 *****)((long)pppppuVar3 + -1);
            LORelease();
            if (pppppuVar3 == (undefined8 *****)0x0) {
              (*(code *)(*ppppppuVar20)[2])(ppppppuVar20);
              func_0x000103462194(ppppppuVar20);
            }
          }
        }
        local_188 = pppppppuVar7;
        func_0x000103462518(local_190);
      }
LAB_1010810e0:
      lVar6 = *(long *)(param_1 + 0x20);
      dVar23 = (double)(**(code **)(**(long **)(param_1 + 0x10) + 0xa0))();
      *(double *)(lVar6 + 0xa8) = dVar23 * 0.001;
      lVar6 = *(long *)(param_1 + 0x20);
      *(undefined8 *)(lVar6 + 0x50) = 0;
      *(undefined8 *)(lVar6 + 0x58) = 0;
      uVar9 = _DAT_1037c4eb0;
      *(undefined8 *)(lVar6 + 0x68) = uRam00000001037c4eb8;
      *(undefined8 *)(lVar6 + 0x60) = uVar9;
      uVar16 = *(undefined4 *)(param_1 + 0x8c);
      iVar5 = *(int *)(param_1 + 0x90);
      puVar11 = (undefined8 *)func_0x000103462620(0x78);
      puVar11[1] = 0;
      puVar11[2] = 0;
      *puVar11 = 0x10400bb40;
      uVar9 = func_0x00010345d898(puVar11 + 3,CONCAT44(iVar5 * iVar4,uVar16),0xd);
      plVar18 = *(long **)(param_1 + 0x38);
      *(undefined8 *)(param_1 + 0x30) = uVar9;
      *(undefined8 **)(param_1 + 0x38) = puVar11;
      if (plVar18 != (long *)0x0) {
        LOAcquire();
        lVar6 = plVar18[1];
        plVar18[1] = lVar6 + -1;
        LORelease();
        if (lVar6 == 0) {
          (**(code **)(*plVar18 + 0x10))(plVar18);
          func_0x000103462194(plVar18);
        }
      }
      *(int *)(param_1 + 0x84) = iVar19;
      *(int *)(param_1 + 0x88) = iVar17;
      if (local_1c0 == 0) {
        uVar16 = 0;
        uVar9 = 0;
      }
      else {
        uVar9 = func_0x000101833558();
        uVar16 = (undefined4)((ulong)uVar9 >> 0x20);
      }
      *(int *)(param_1 + 0xa8) = (int)uVar9;
      *(undefined4 *)(param_1 + 0xac) = uVar16;
      lVar6 = func_0x0001034622d8();
      if (lVar6 - lVar21 < 31000000) goto LAB_1010812e8;
      lVar13 = 0x103d1e53f;
      lVar14 = func_0x000103463bb0(0x103d1e53f,0x2f);
      if (lVar14 != 0) {
        lVar13 = lVar14 + 1;
      }
      pppppppuVar7 = (undefined8 *******)func_0x000103463b5c(lVar13);
      if ((undefined8 *******)0x7ffffffffffffff7 < pppppppuVar7) {
        func_0x000100016eb8(&local_190);
        goto LAB_101081378;
      }
      if (pppppppuVar7 < (undefined8 *******)0x17) {
        uStack_180 = CONCAT17((char)pppppppuVar7,(undefined7)uStack_180);
        pppppppuVar8 = &local_190;
        if (pppppppuVar7 != (undefined8 *******)0x0) goto LAB_10108123c;
      }
      else {
        uVar10 = ((ulong)pppppppuVar7 & 0xfffffffffffffff8) + 8;
        if (((ulong)pppppppuVar7 | 7) != 0x17) {
          uVar10 = (ulong)pppppppuVar7 | 7;
        }
        pppppppuVar8 = (undefined8 *******)func_0x000103462620(uVar10 + 1);
        uStack_180 = uVar10 + 1 | 0x8000000000000000;
        local_190 = pppppppuVar8;
        local_188 = pppppppuVar7;
LAB_10108123c:
        func_0x00010346343c(pppppppuVar8,lVar13,pppppppuVar7);
      }
      *(undefined1 *)((long)pppppppuVar8 + (long)pppppppuVar7) = 0;
      local_1a0 = CONCAT17(0x11,(undefined7)local_1a0);
      plStack_1a8 = plRam0000000103d1e5d8;
      local_1b0 = _DAT_103d1e5d0;
      local_1a0 = CONCAT62(local_1a0._2_6_,0x65);
      func_0x00010002b8a0(&local_178,&local_190,&local_1b0,0xa9,2);
      func_0x000100016bb0(&local_178,0x103d1b4bf,0x15);
      func_0x00010346202c(&local_178,(lVar6 - lVar21) / 1000000);
      func_0x00010002bfd0(&local_178);
      if ((long)local_1a0 < 0) {
        func_0x000103462518(local_1b0);
      }
      if ((long)uStack_180 < 0) {
        func_0x000103462518(local_190);
      }
LAB_1010812e8:
      plVar18 = local_1b8;
      if (local_1b8 == (long *)0x0) {
        return;
      }
      LOAcquire();
      lVar21 = local_1b8[1];
      local_1b8[1] = lVar21 + -1;
      LORelease();
      if (lVar21 != 0) {
        return;
      }
      (**(code **)(*local_1b8 + 0x10))(local_1b8);
      func_0x000103462194(plVar18);
      return;
    }
    (**(code **)(**(long **)(param_1 + 0x10) + 0x98))(&local_1f0);
    local_200 = *(undefined4 *)(param_1 + 0x8c);
    uStack_1fc = *(undefined4 *)(param_1 + 0x90);
    local_208 = iVar19;
    iStack_204 = iVar17;
    local_1f8 = local_200;
    uStack_1f4 = uStack_1fc;
    func_0x000101b53d98(*(undefined8 *)(param_1 + 0x20),param_2,&local_1f8,&local_200,&local_208,
                        &local_1f0,1,*(undefined1 *)(param_1 + 0xa4));
    lVar6 = 0x103d1e53f;
    lVar13 = func_0x000103463bb0(0x103d1e53f,0x2f);
    if (lVar13 != 0) {
      lVar6 = lVar13 + 1;
    }
    pppppppuVar7 = (undefined8 *******)func_0x000103463b5c(lVar6);
    if (pppppppuVar7 < (undefined8 *******)0x7ffffffffffffff8) {
      if (pppppppuVar7 < (undefined8 *******)0x17) {
        uStack_180 = CONCAT17((char)pppppppuVar7,(undefined7)uStack_180);
        pppppppuVar8 = &local_190;
        if (pppppppuVar7 != (undefined8 *******)0x0) goto LAB_101080edc;
      }
      else {
        uVar10 = ((ulong)pppppppuVar7 & 0xfffffffffffffff8) + 8;
        if (((ulong)pppppppuVar7 | 7) != 0x17) {
          uVar10 = (ulong)pppppppuVar7 | 7;
        }
        pppppppuVar8 = (undefined8 *******)func_0x000103462620(uVar10 + 1);
        uStack_180 = uVar10 + 1 | 0x8000000000000000;
        local_190 = pppppppuVar8;
        local_188 = pppppppuVar7;
LAB_101080edc:
        func_0x00010346343c(pppppppuVar8,lVar6,pppppppuVar7);
      }
      *(undefined1 *)((long)pppppppuVar8 + (long)pppppppuVar7) = 0;
      local_1a0 = CONCAT17(0x11,(undefined7)local_1a0);
      plStack_1a8 = plRam0000000103d1e5d8;
      local_1b0 = _DAT_103d1e5d0;
      local_1a0 = CONCAT62(local_1a0._2_6_,0x65);
      func_0x00010002b8a0(&local_178,&local_190,&local_1b0,0x9e,1);
      func_0x000100016bb0(&local_178,0x103d1b499,0x25);
      func_0x00010002bfd0(&local_178);
      if ((long)local_1a0 < 0) {
        func_0x000103462518(local_1b0);
      }
      plVar18 = plStack_1e8;
      if ((long)uStack_180 < 0) {
        func_0x000103462518(local_190);
        plVar18 = plStack_1e8;
      }
      plStack_1e8 = plVar18;
      if (plVar18 != (long *)0x0) {
        LOAcquire();
        lVar6 = plVar18[1];
        plVar18[1] = lVar6 + -1;
        LORelease();
        if (lVar6 == 0) {
          (**(code **)(*plVar18 + 0x10))(plVar18);
          func_0x000103462194(plVar18);
        }
      }
      goto LAB_1010810e0;
    }
  }
  else {
    plVar18 = *(long **)(param_1 + 0x18);
    if (plVar18 == (long *)0x0) {
LAB_1010806ac:
      lVar21 = *(long *)(param_1 + 0x20);
    }
    else {
      plVar1 = plVar18 + 1;
      *plVar1 = *plVar1 + 1;
      LOAcquire();
      lVar21 = *plVar1;
      *plVar1 = lVar21 + -1;
      LORelease();
      if (lVar21 != 0) goto LAB_1010806ac;
      (**(code **)(*plVar18 + 0x10))(plVar18);
      func_0x000103462194(plVar18);
      lVar21 = *(long *)(param_1 + 0x20);
    }
    if (lVar21 == 0) {
      return;
    }
    plVar18 = *(long **)(param_1 + 0x28);
    *(undefined8 *)(param_1 + 0x20) = 0;
    *(undefined8 *)(param_1 + 0x28) = 0;
    if (plVar18 != (long *)0x0) {
      LOAcquire();
      lVar21 = plVar18[1];
      plVar18[1] = lVar21 + -1;
      LORelease();
      if (lVar21 == 0) {
        (**(code **)(*plVar18 + 0x10))(plVar18);
        func_0x000103462194(plVar18);
      }
    }
    if (*(char *)(param_1 + 0x80) != '\0') {
      *(undefined1 *)(param_1 + 0x80) = 0;
    }
    lVar21 = 0x103d1e53f;
    lVar6 = func_0x000103463bb0(0x103d1e53f,0x2f);
    if (lVar6 != 0) {
      lVar21 = lVar6 + 1;
    }
    pppppppuVar7 = (undefined8 *******)func_0x000103463b5c(lVar21);
    if (pppppppuVar7 < (undefined8 *******)0x7ffffffffffffff8) {
      if (pppppppuVar7 < (undefined8 *******)0x17) {
        uStack_180 = CONCAT17((char)pppppppuVar7,(undefined7)uStack_180);
        pppppppuVar8 = &local_190;
        if (pppppppuVar7 == (undefined8 *******)0x0) goto LAB_101080784;
      }
      else {
        uVar10 = ((ulong)pppppppuVar7 & 0xfffffffffffffff8) + 8;
        if (((ulong)pppppppuVar7 | 7) != 0x17) {
          uVar10 = (ulong)pppppppuVar7 | 7;
        }
        pppppppuVar8 = (undefined8 *******)func_0x000103462620(uVar10 + 1);
        uStack_180 = uVar10 + 1 | 0x8000000000000000;
        local_190 = pppppppuVar8;
        local_188 = pppppppuVar7;
      }
      func_0x00010346343c(pppppppuVar8,lVar21,pppppppuVar7);
LAB_101080784:
      *(undefined1 *)((long)pppppppuVar8 + (long)pppppppuVar7) = 0;
      local_1a0 = CONCAT17(0x11,(undefined7)local_1a0);
      plStack_1a8 = plRam0000000103d1e5d8;
      local_1b0 = _DAT_103d1e5d0;
      local_1a0 = CONCAT62(local_1a0._2_6_,0x65);
      func_0x00010002b8a0(&local_178,&local_190,&local_1b0,0x60,1);
      func_0x000100016bb0(&local_178,0x103d1e5e2,0x15);
      func_0x00010002bfd0(&local_178);
      if ((long)local_1a0 < 0) {
        func_0x000103462518(local_1b0);
      }
      if ((long)uStack_180 < 0) {
        func_0x000103462518(local_190);
      }
      return;
    }
    func_0x000100016eb8(&local_190);
  }
  func_0x000100016eb8(&local_190);
LAB_101081378:
                    /* WARNING: Does not return */
  pcVar2 = (code *)SoftwareBreakpoint(1,0x10108137c);
  (*pcVar2)();
}

