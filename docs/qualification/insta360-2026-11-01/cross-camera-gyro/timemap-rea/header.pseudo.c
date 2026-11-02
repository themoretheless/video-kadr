
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(long param_1)

{
  ulong uVar1;
  uint uVar2;
  code *pcVar3;
  int iVar4;
  long lVar5;
  undefined8 *puVar6;
  long lVar7;
  ulong uVar8;
  undefined8 ******ppppppuVar9;
  undefined7 *puVar10;
  undefined8 uVar11;
  long *plVar12;
  undefined8 uVar13;
  undefined7 local_200;
  undefined1 uStack_1f9;
  undefined3 uStack_1f8;
  undefined1 local_1f5;
  char local_1e9;
  undefined7 local_1e8;
  undefined4 uStack_1e1;
  undefined1 uStack_1dd;
  undefined4 uStack_1dc;
  undefined8 uStack_1d8;
  undefined7 local_1b8;
  undefined1 uStack_1b1;
  undefined3 uStack_1b0;
  undefined1 uStack_1ad;
  undefined4 uStack_1ac;
  undefined8 uStack_1a8;
  undefined8 *****local_1a0;
  ulong local_198;
  undefined8 uStack_190;
  undefined1 auStack_170 [272];
  
  lVar7 = param_1 + 0x40;
  iVar4 = func_0x000101bdff20(lVar7);
  if ((0 < iVar4) && (lVar5 = func_0x000101bdff08(lVar7), lVar5 != 0)) {
    puVar6 = (undefined8 *)func_0x000101bdff08(lVar7);
    uVar11 = puVar6[2];
    uVar13 = *puVar6;
    *(undefined8 *)(param_1 + 0x10) = puVar6[1];
    *(undefined8 *)(param_1 + 8) = uVar13;
    *(undefined8 *)(param_1 + 0x18) = uVar11;
    if (((ulong)*(uint *)(param_1 + 8) == (ulong)*(uint *)(param_1 + 0xc) * 0x20) &&
       ((ulong)*(uint *)(param_1 + 0x10) == (ulong)*(uint *)(param_1 + 0x14) * 0x10)) {
      func_0x000101be0128(&local_1a0);
      uVar11 = func_0x000101be035c(&local_1a0);
      lVar5 = func_0x000101bdff08(lVar7);
      func_0x000103463430(uVar11,lVar5 + 0x18,*(uint *)(param_1 + 8));
      puVar6 = (undefined8 *)func_0x000103462620(0x60);
      puVar6[2] = 0;
      *puVar6 = 0x104002ce0;
      puVar6[1] = 0;
      puVar6[3] = 0x1040a24a8;
      func_0x000101bdff54(puVar6 + 4,&local_1a0);
      uVar11 = _DAT_103b1be70;
      puVar6[0xb] = uRam0000000103b1be78;
      puVar6[10] = uVar11;
      plVar12 = *(long **)(param_1 + 0x28);
      *(undefined8 **)(param_1 + 0x20) = puVar6 + 3;
      *(undefined8 **)(param_1 + 0x28) = puVar6;
      if (plVar12 != (long *)0x0) {
        LOAcquire();
        lVar5 = plVar12[1];
        plVar12[1] = lVar5 + -1;
        LORelease();
        if (lVar5 == 0) {
          (**(code **)(*plVar12 + 0x10))(plVar12);
          func_0x000103462194(plVar12);
        }
      }
      lVar5 = (**(code **)(**(long **)(param_1 + 0x20) + 0x58))();
      if (lVar5 != 0) {
        func_0x000101be0128(&local_1e8,*(undefined4 *)(param_1 + 0x10));
        uVar2 = *(uint *)(param_1 + 8);
        uVar11 = func_0x000101be035c(&local_1e8);
        lVar7 = func_0x000101bdff08(lVar7);
        func_0x000103463430(uVar11,lVar7 + (ulong)uVar2 + 0x18,*(undefined4 *)(param_1 + 0x10));
        puVar6 = (undefined8 *)func_0x000103462620(0x60);
        puVar6[1] = 0;
        puVar6[2] = 0;
        *puVar6 = 0x104002ce0;
        puVar6[3] = 0x1040a24a8;
        func_0x000101bdff54(puVar6 + 4,&local_1e8);
        uVar11 = _DAT_103b19b70;
        puVar6[0xb] = uRam0000000103b19b78;
        puVar6[10] = uVar11;
        plVar12 = *(long **)(param_1 + 0x38);
        *(undefined8 **)(param_1 + 0x30) = puVar6 + 3;
        *(undefined8 **)(param_1 + 0x38) = puVar6;
        if (plVar12 != (long *)0x0) {
          LOAcquire();
          lVar7 = plVar12[1];
          plVar12[1] = lVar7 + -1;
          LORelease();
          if (lVar7 == 0) {
            (**(code **)(*plVar12 + 0x10))(plVar12);
            func_0x000103462194(plVar12);
          }
        }
        lVar7 = (**(code **)(**(long **)(param_1 + 0x30) + 0x58))();
        if (lVar7 == 0) {
          lVar7 = 0x103d68fdb;
          lVar5 = func_0x000103463bb0(0x103d68fdb,0x2f);
          if (lVar5 != 0) {
            lVar7 = lVar5 + 1;
          }
          uVar8 = func_0x000103463b5c(lVar7);
          if (0x7ffffffffffffff7 < uVar8) {
            func_0x000100016eb8(&local_1b8);
            goto LAB_10186985c;
          }
          if (uVar8 < 0x17) {
            uStack_1a8 = CONCAT17((char)uVar8,(undefined7)uStack_1a8);
            puVar10 = &local_1b8;
            if (uVar8 != 0) goto LAB_101869798;
          }
          else {
            uVar1 = (uVar8 & 0xfffffffffffffff8) + 8;
            if ((uVar8 | 7) != 0x17) {
              uVar1 = uVar8 | 7;
            }
            puVar10 = (undefined7 *)func_0x000103462620(uVar1 + 1);
            uStack_1a8 = uVar1 + 1 | 0x8000000000000000;
            uStack_1b0 = (undefined3)uVar8;
            uStack_1ad = (undefined1)(uVar8 >> 0x18);
            uStack_1ac = (undefined4)(uVar8 >> 0x20);
            local_1b8 = SUB87(puVar10,0);
            uStack_1b1 = (undefined1)((ulong)puVar10 >> 0x38);
LAB_101869798:
            func_0x00010346343c(puVar10,lVar7,uVar8);
          }
          *(undefined1 *)((long)puVar10 + uVar8) = 0;
          local_1e9 = '\v';
          local_200 = (undefined7)_DAT_103d69056;
          uStack_1f9 = (undefined1)((ulong)_DAT_103d69056 >> 0x38);
          _uStack_1f9 = CONCAT31(0x726564,uStack_1f9);
          local_1f5 = 0;
          func_0x00010002b8a0(auStack_170,&local_1b8,&local_200,0x46,3);
          func_0x000100016bb0(auStack_170,0x103d690ba,0x1d);
          func_0x00010002bfd0(auStack_170);
          if (local_1e9 < '\0') {
            func_0x000103462518(CONCAT17(uStack_1f9,local_200));
          }
          if ((long)uStack_1a8 < 0) {
            func_0x000103462518(CONCAT17(uStack_1b1,local_1b8));
          }
          *(undefined1 *)(param_1 + 0x58) = 0;
        }
        func_0x000101be0260(&local_1e8);
        goto LAB_101869834;
      }
      lVar7 = 0x103d68fdb;
      lVar5 = func_0x000103463bb0(0x103d68fdb,0x2f);
      if (lVar5 != 0) {
        lVar7 = lVar5 + 1;
      }
      uVar8 = func_0x000103463b5c(lVar7);
      if (0x7ffffffffffffff7 < uVar8) goto LAB_101869848;
      if (uVar8 < 0x17) {
        uStack_1d8 = CONCAT17((char)uVar8,(undefined7)uStack_1d8);
        puVar10 = &local_1e8;
        if (uVar8 != 0) goto LAB_1018696d0;
      }
      else {
        uVar1 = (uVar8 & 0xfffffffffffffff8) + 8;
        if ((uVar8 | 7) != 0x17) {
          uVar1 = uVar8 | 7;
        }
        puVar10 = (undefined7 *)func_0x000103462620(uVar1 + 1);
        uStack_1d8 = uVar1 + 1 | 0x8000000000000000;
        uStack_1e1._1_3_ = (undefined3)uVar8;
        uStack_1dd = (undefined1)(uVar8 >> 0x18);
        uStack_1dc = (undefined4)(uVar8 >> 0x20);
        local_1e8 = SUB87(puVar10,0);
        uStack_1e1._0_1_ = (undefined1)((ulong)puVar10 >> 0x38);
LAB_1018696d0:
        func_0x00010346343c(puVar10,lVar7,uVar8);
      }
      *(undefined1 *)((long)puVar10 + uVar8) = 0;
      uStack_1a8 = CONCAT17(0xb,(undefined7)uStack_1a8);
      local_1b8 = (undefined7)_DAT_103d69056;
      uStack_1b1 = (undefined1)((ulong)_DAT_103d69056 >> 0x38);
      _uStack_1b1 = CONCAT31(0x726564,uStack_1b1);
      uStack_1ad = 0;
      func_0x00010002b8a0(auStack_170,&local_1e8,&local_1b8,0x3b,3);
      func_0x000100016bb0(auStack_170,0x103d6909a,0x1f);
      func_0x00010002bfd0(auStack_170);
      if ((long)uStack_1a8 < 0) {
        func_0x000103462518(CONCAT17(uStack_1b1,local_1b8));
      }
      if ((long)uStack_1d8 < 0) {
        func_0x000103462518(CONCAT17((undefined1)uStack_1e1,local_1e8));
      }
      *(undefined1 *)(param_1 + 0x58) = 0;
LAB_101869834:
      func_0x000101be0260(&local_1a0);
      return;
    }
    lVar7 = 0x103d68fdb;
    lVar5 = func_0x000103463bb0(0x103d68fdb,0x2f);
    if (lVar5 != 0) {
      lVar7 = lVar5 + 1;
    }
    uVar8 = func_0x000103463b5c(lVar7);
    if (0x7ffffffffffffff7 < uVar8) goto LAB_101869840;
    if (uVar8 < 0x17) {
      uStack_190 = CONCAT17((char)uVar8,(undefined7)uStack_190);
      ppppppuVar9 = &local_1a0;
      if (uVar8 != 0) goto LAB_101869590;
    }
    else {
      uVar1 = (uVar8 & 0xfffffffffffffff8) + 8;
      if ((uVar8 | 7) != 0x17) {
        uVar1 = uVar8 | 7;
      }
      ppppppuVar9 = (undefined8 ******)func_0x000103462620(uVar1 + 1);
      uStack_190 = uVar1 + 1 | 0x8000000000000000;
      local_1a0 = ppppppuVar9;
      local_198 = uVar8;
LAB_101869590:
      func_0x00010346343c(ppppppuVar9,lVar7,uVar8);
    }
    *(undefined1 *)((long)ppppppuVar9 + uVar8) = 0;
    uStack_1d8 = CONCAT17(0xb,(undefined7)uStack_1d8);
    local_1e8 = (undefined7)_DAT_103d69056;
    uStack_1e1._0_1_ = (undefined1)((ulong)_DAT_103d69056 >> 0x38);
    uStack_1e1 = CONCAT31(0x726564,(undefined1)uStack_1e1);
    uStack_1dd = 0;
    func_0x00010002b8a0(auStack_170,&local_1a0,&local_1e8,0x30,3);
    func_0x000100016bb0(auStack_170,0x103d6907d,0x1c);
    goto LAB_1018695f8;
  }
  lVar7 = 0x103d68fdb;
  lVar5 = func_0x000103463bb0(0x103d68fdb,0x2f);
  if (lVar5 != 0) {
    lVar7 = lVar5 + 1;
  }
  uVar8 = func_0x000103463b5c(lVar7);
  if (0x7ffffffffffffff7 < uVar8) {
LAB_101869840:
    func_0x000100016eb8(&local_1a0);
LAB_101869848:
    func_0x000100016eb8(&local_1e8);
LAB_10186985c:
                    /* WARNING: Does not return */
    pcVar3 = (code *)SoftwareBreakpoint(1,0x101869860);
    (*pcVar3)();
  }
  if (uVar8 < 0x17) {
    uStack_190 = CONCAT17((char)uVar8,(undefined7)uStack_190);
    ppppppuVar9 = &local_1a0;
    if (uVar8 != 0) goto LAB_1018694a4;
  }
  else {
    uVar1 = (uVar8 & 0xfffffffffffffff8) + 8;
    if ((uVar8 | 7) != 0x17) {
      uVar1 = uVar8 | 7;
    }
    ppppppuVar9 = (undefined8 ******)func_0x000103462620(uVar1 + 1);
    uStack_190 = uVar1 + 1 | 0x8000000000000000;
    local_1a0 = ppppppuVar9;
    local_198 = uVar8;
LAB_1018694a4:
    func_0x00010346343c(ppppppuVar9,lVar7,uVar8);
  }
  *(undefined1 *)((long)ppppppuVar9 + uVar8) = 0;
  uStack_1d8 = CONCAT17(0xb,(undefined7)uStack_1d8);
  local_1e8 = (undefined7)_DAT_103d69056;
  uStack_1e1._0_1_ = (undefined1)((ulong)_DAT_103d69056 >> 0x38);
  uStack_1e1 = CONCAT31(0x726564,(undefined1)uStack_1e1);
  uStack_1dd = 0;
  func_0x00010002b8a0(auStack_170,&local_1a0,&local_1e8,0x28,3);
  func_0x000100016bb0(auStack_170,0x103d69062,0x1a);
LAB_1018695f8:
  func_0x00010002bfd0(auStack_170);
  if ((long)uStack_1d8 < 0) {
    func_0x000103462518(CONCAT17((undefined1)uStack_1e1,local_1e8));
  }
  if ((long)uStack_190 < 0) {
    func_0x000103462518(local_1a0);
  }
  *(undefined1 *)(param_1 + 0x58) = 0;
  return;
}

