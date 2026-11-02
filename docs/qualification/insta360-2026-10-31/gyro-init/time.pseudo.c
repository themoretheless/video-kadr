
void entry(undefined8 param_1,long param_2,undefined8 param_3,long *param_4,long *param_5,
          undefined8 *param_6)

{
  undefined8 uVar1;
  long lVar2;
  long *plVar3;
  long lVar4;
  undefined8 *puVar5;
  long *plVar6;
  long *plVar7;
  long *local_68;
  long *local_60;
  long *local_58;
  undefined8 local_50;
  undefined1 uStack_41;
  
  lVar4 = *param_4;
  lVar2 = param_4[1];
  if (lVar2 != 0) {
    *(long *)(lVar2 + 8) = *(long *)(lVar2 + 8) + 1;
  }
  plVar7 = *(long **)(param_2 + 0xa8);
  *(long *)(param_2 + 0xa0) = lVar4;
  *(long *)(param_2 + 0xa8) = lVar2;
  local_50 = param_1;
  if (plVar7 != (long *)0x0) {
    LOAcquire();
    lVar4 = plVar7[1];
    plVar7[1] = lVar4 + -1;
    LORelease();
    if (lVar4 == 0) {
      (**(code **)(*plVar7 + 0x10))(plVar7);
      func_0x000103462194(plVar7);
    }
  }
  lVar4 = *param_5;
  lVar2 = param_5[1];
  if (lVar2 != 0) {
    *(long *)(lVar2 + 8) = *(long *)(lVar2 + 8) + 1;
  }
  plVar7 = *(long **)(param_2 + 0xb8);
  *(long *)(param_2 + 0xb0) = lVar4;
  *(long *)(param_2 + 0xb8) = lVar2;
  if (plVar7 != (long *)0x0) {
    LOAcquire();
    lVar4 = plVar7[1];
    plVar7[1] = lVar4 + -1;
    LORelease();
    if (lVar4 == 0) {
      (**(code **)(*plVar7 + 0x10))(plVar7);
      func_0x000103462194(plVar7);
    }
  }
  uVar1 = *param_6;
  lVar4 = param_6[1];
  if (lVar4 != 0) {
    *(long *)(lVar4 + 8) = *(long *)(lVar4 + 8) + 1;
  }
  plVar7 = *(long **)(param_2 + 200);
  *(undefined8 *)(param_2 + 0xc0) = uVar1;
  *(long *)(param_2 + 200) = lVar4;
  if (plVar7 != (long *)0x0) {
    LOAcquire();
    lVar4 = plVar7[1];
    plVar7[1] = lVar4 + -1;
    LORelease();
    if (lVar4 == 0) {
      (**(code **)(*plVar7 + 0x10))(plVar7);
      func_0x000103462194(plVar7);
      lVar4 = *param_4;
      goto joined_r0x000101837e8c;
    }
  }
  lVar4 = *param_4;
joined_r0x000101837e8c:
  if (lVar4 == 0) {
    local_58 = (long *)0x0;
    local_60 = (long *)0x0;
    local_68 = (long *)0x0;
    lVar4 = *param_5;
  }
  else {
    local_58 = (long *)0x0;
    local_60 = (long *)0x0;
    local_68 = (long *)0x0;
    local_60 = (long *)func_0x0001000332f4(&local_68,param_4);
    lVar4 = *param_5;
  }
  if (lVar4 != 0) {
    if (local_60 < local_58) {
      *local_60 = lVar4;
      lVar4 = param_5[1];
      local_60[1] = lVar4;
      if (lVar4 != 0) {
        *(long *)(lVar4 + 8) = *(long *)(lVar4 + 8) + 1;
      }
      local_60 = local_60 + 2;
    }
    else {
      local_60 = (long *)func_0x0001000332f4(&local_68,param_5);
    }
  }
  puVar5 = (undefined8 *)func_0x000103462620(0xb8);
  puVar5[2] = 0;
  *puVar5 = 0x1040a3068;
  puVar5[1] = 0;
  func_0x000101845218(&uStack_41,puVar5 + 3,param_3,&local_50,&local_68,param_2 + 0xc0);
  *(undefined8 **)(param_2 + 0x220) = puVar5 + 3;
  plVar7 = *(long **)(param_2 + 0x228);
  *(undefined8 **)(param_2 + 0x228) = puVar5;
  if (plVar7 != (long *)0x0) {
    LOAcquire();
    lVar4 = plVar7[1];
    plVar7[1] = lVar4 + -1;
    LORelease();
    if (lVar4 == 0) {
      (**(code **)(*plVar7 + 0x10))(plVar7);
      func_0x000103462194(plVar7);
    }
  }
  plVar3 = local_68;
  plVar7 = local_60;
  if (local_68 != (long *)0x0) {
    for (; plVar7 != plVar3; plVar7 = plVar7 + -2) {
      plVar6 = (long *)plVar7[-1];
      if (plVar6 != (long *)0x0) {
        LOAcquire();
        lVar4 = plVar6[1];
        plVar6[1] = lVar4 + -1;
        LORelease();
        if (lVar4 == 0) {
          (**(code **)(*plVar6 + 0x10))(plVar6);
          func_0x000103462194(plVar6);
        }
      }
    }
    local_60 = plVar3;
    func_0x000103462518(local_68);
  }
  return;
}

