
void entry(int *param_1,undefined8 param_2,undefined8 param_3,int param_4,undefined8 param_5)

{
  long lVar1;
  undefined8 *puVar2;
  undefined8 uVar3;
  long *plVar4;
  undefined1 auStack_60 [12];
  undefined4 local_54;
  
  local_54 = (undefined4)param_2;
  func_0x000102e66350(auStack_60,0x103d66642);
  *param_1 = param_4;
  param_1[2] = (int)param_5;
  puVar2 = (undefined8 *)func_0x000103462620(0x30);
  puVar2[2] = 0;
  *puVar2 = 0x1040a2fc8;
  puVar2[1] = 0;
  uVar3 = func_0x0001027b7040(puVar2 + 3,param_2,param_5,1);
  plVar4 = *(long **)(param_1 + 0x44);
  *(undefined8 *)(param_1 + 0x42) = uVar3;
  *(undefined8 **)(param_1 + 0x44) = puVar2;
  if (plVar4 != (long *)0x0) {
    LOAcquire();
    lVar1 = plVar4[1];
    plVar4[1] = lVar1 + -1;
    LORelease();
    if (lVar1 == 0) {
      (**(code **)(*plVar4 + 0x10))(plVar4);
      func_0x000103462194(plVar4);
    }
  }
  if ((int)param_3 != -1) {
    puVar2 = (undefined8 *)func_0x000103462620(0x30);
    puVar2[1] = 0;
    puVar2[2] = 0;
    *puVar2 = 0x1040a2fc8;
    uVar3 = func_0x0001027b7040(puVar2 + 3,param_3,param_5,1);
    plVar4 = *(long **)(param_1 + 0x48);
    *(undefined8 *)(param_1 + 0x46) = uVar3;
    *(undefined8 **)(param_1 + 0x48) = puVar2;
    if (plVar4 != (long *)0x0) {
      LOAcquire();
      lVar1 = plVar4[1];
      plVar4[1] = lVar1 + -1;
      LORelease();
      if (lVar1 == 0) {
        (**(code **)(*plVar4 + 0x10))(plVar4);
        func_0x000103462194(plVar4);
      }
    }
  }
  if (param_4 - 5U < 2) {
    puVar2 = (undefined8 *)func_0x000103462620(0x148);
    puVar2[2] = 0;
    *puVar2 = 0x104001448;
    puVar2[1] = 0;
    uVar3 = func_0x0001028243d8(puVar2 + 3,&local_54,1);
    *(undefined8 *)(param_1 + 0x84) = uVar3;
    plVar4 = *(long **)(param_1 + 0x86);
    *(undefined8 **)(param_1 + 0x86) = puVar2;
    if (plVar4 != (long *)0x0) {
      LOAcquire();
      lVar1 = plVar4[1];
      plVar4[1] = lVar1 + -1;
      LORelease();
      if (lVar1 == 0) {
        (**(code **)(*plVar4 + 0x10))(plVar4);
        func_0x000103462194(plVar4);
      }
    }
    uVar3 = *(undefined8 *)(*(long *)(param_1 + 0x84) + 0xf8);
    lVar1 = *(long *)(*(long *)(param_1 + 0x84) + 0x100);
    if (lVar1 != 0) {
      *(long *)(lVar1 + 8) = *(long *)(lVar1 + 8) + 1;
    }
    plVar4 = *(long **)(param_1 + 0x44);
    *(undefined8 *)(param_1 + 0x42) = uVar3;
    *(long *)(param_1 + 0x44) = lVar1;
    if (plVar4 != (long *)0x0) {
      LOAcquire();
      lVar1 = plVar4[1];
      plVar4[1] = lVar1 + -1;
      LORelease();
      if (lVar1 == 0) {
        (**(code **)(*plVar4 + 0x10))(plVar4);
        func_0x000103462194(plVar4);
      }
    }
  }
  func_0x000102e66360(auStack_60);
  return;
}

