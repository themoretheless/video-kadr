
undefined4 entry(long param_1)

{
  ulong uVar1;
  int iVar2;
  undefined4 uVar3;
  undefined4 *local_1f0;
  undefined4 *local_1e8;
  undefined8 local_1e0;
  undefined1 auStack_1d8 [424];
  
  uVar1 = *(ulong *)(param_1 + 8);
  if (-1 < (char)*(byte *)(param_1 + 0x17)) {
    uVar1 = (ulong)*(byte *)(param_1 + 0x17);
  }
  if (uVar1 == 0) {
    return 0xffffffff;
  }
  func_0x0001028d0594(auStack_1d8);
  iVar2 = func_0x0001028d083c(auStack_1d8,param_1);
  func_0x00010021e050(auStack_1d8);
  if (iVar2 != 0) {
    return 0xffffffff;
  }
  func_0x0001028d0594(auStack_1d8);
  func_0x0001028d05c4(auStack_1d8,param_1);
  local_1f0 = (undefined4 *)0x0;
  local_1e8 = (undefined4 *)0x0;
  local_1e0 = 0;
  func_0x0001028d77c0(auStack_1d8,&local_1f0);
  if (local_1f0 == local_1e8) {
    uVar3 = 0xffffffff;
    if (local_1f0 == (undefined4 *)0x0) goto LAB_1018b2248;
  }
  else {
    uVar3 = *local_1f0;
  }
  local_1e8 = local_1f0;
  func_0x000103462518();
LAB_1018b2248:
  func_0x00010021e050(auStack_1d8);
  return uVar3;
}

