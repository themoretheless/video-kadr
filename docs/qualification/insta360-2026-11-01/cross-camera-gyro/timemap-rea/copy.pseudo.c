
void entry(undefined8 *param_1,long param_2)

{
  long *plVar1;
  long lVar2;
  int iVar3;
  undefined8 uVar4;
  long *plVar5;
  long *plVar6;
  undefined1 auStack_78 [24];
  undefined8 local_60;
  long *local_58;
  undefined1 auStack_4c [8];
  int local_44;
  
  if ((*(int *)(param_2 + 0xb4) == 0) ||
     (iVar3 = func_0x000101b5e940(*(undefined8 *)(param_2 + 0xb8),0x80,auStack_4c),
     iVar3 != 0 || local_44 < 1)) {
    *param_1 = 0;
    param_1[1] = 0;
  }
  else {
    local_60 = 0;
    local_58 = (long *)0x0;
    uVar4 = func_0x000103462620(local_44);
    func_0x000103462bf0(uVar4,local_44);
    iVar3 = func_0x000101b5e3a4(*(undefined8 *)(param_2 + 0xb8),0x80,uVar4,local_44,0);
    if (iVar3 == 0) {
      func_0x000101bdfd30(auStack_78,uVar4,local_44);
      plVar5 = (long *)func_0x000103462620(0x78);
      plVar6 = plVar5 + 1;
      *plVar6 = 0;
      plVar5[2] = 0;
      *plVar5 = 0x1040a1a28;
      plVar1 = plVar5 + 3;
      func_0x000101868fb0(plVar1,auStack_78);
      iVar3 = func_0x0001018691d8(plVar1);
      if (iVar3 == 0) {
        *param_1 = 0;
        param_1[1] = 0;
        LOAcquire();
        lVar2 = *plVar6;
        *plVar6 = lVar2 + -1;
        LORelease();
        if (lVar2 == 0) {
          (**(code **)(*plVar5 + 0x10))(plVar5);
          func_0x000103462194(plVar5);
        }
      }
      else {
        *param_1 = plVar1;
        param_1[1] = plVar5;
      }
      func_0x000101bdfe30(auStack_78);
      func_0x000103462518(uVar4);
      plVar1 = local_58;
    }
    else {
      *param_1 = 0;
      param_1[1] = 0;
      func_0x000103462518(uVar4);
      plVar1 = local_58;
    }
    if (plVar1 != (long *)0x0) {
      LOAcquire();
      lVar2 = plVar1[1];
      plVar1[1] = lVar2 + -1;
      LORelease();
      if (lVar2 == 0) {
        local_58 = plVar1;
        (**(code **)(*plVar1 + 0x10))(plVar1);
        func_0x000103462194(plVar1);
        return;
      }
    }
  }
  return;
}

