
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

int entry(long param_1,int param_2,undefined8 param_3)

{
  ulong uVar1;
  int iVar2;
  undefined8 uVar3;
  undefined8 uVar4;
  undefined8 local_1e8;
  undefined2 local_1e0;
  undefined8 local_1dc;
  undefined8 local_1d0;
  
  uVar1 = *(ulong *)(param_1 + 8);
  if (-1 < (char)*(byte *)(param_1 + 0x17)) {
    uVar1 = (ulong)*(byte *)(param_1 + 0x17);
  }
  if (uVar1 == 0) {
    return -1;
  }
  func_0x0001028d0594(&local_1e8);
  iVar2 = func_0x0001028d083c(&local_1e8,param_1);
  func_0x00010021e050(&local_1e8);
  if (iVar2 == 0) {
    uVar3 = func_0x0001018b218c(param_1);
    uVar4 = func_0x0001018b51d4(param_1);
    local_1e8 = 0;
    local_1e0 = 0;
    local_1dc = _DAT_1037a8f08;
    local_1d0 = 0;
    iVar2 = func_0x0001018b42d8(&local_1e8,uVar3,uVar4,param_3);
    if (iVar2 == 0) {
      return -1;
    }
    if (param_2 != 0) {
      if (local_1dc._4_4_ == 0x11) {
        return 0x13;
      }
      if (local_1dc._4_4_ == 0x24) {
        return 0x25;
      }
      if (local_1dc._4_4_ != 0x12) {
        iVar2 = 0x26;
        if (local_1dc._4_4_ != 0x20) {
          iVar2 = local_1dc._4_4_;
        }
        return iVar2;
      }
      return 0x14;
    }
  }
  else {
    local_1dc._4_4_ = -1;
  }
  return local_1dc._4_4_;
}

