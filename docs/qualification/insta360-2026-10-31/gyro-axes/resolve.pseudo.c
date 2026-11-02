
undefined8 entry(long param_1)

{
  char cVar1;
  bool bVar2;
  undefined8 uVar3;
  ulong local_60;
  undefined8 uStack_58;
  ulong local_50;
  ulong local_40;
  undefined8 uStack_38;
  ulong local_30;
  
  if (*(char *)(param_1 + 0x108) == '\0') {
    local_30 = local_30 & 0xffffffffffffff;
    local_40 = local_40 & 0xffffffffffffff00;
    cVar1 = *(char *)(param_1 + 0x251);
  }
  else if (*(char *)(param_1 + 0x107) < '\0') {
    func_0x000101e45618(&local_40,*(undefined8 *)(param_1 + 0xf0),*(undefined8 *)(param_1 + 0xf8));
    cVar1 = *(char *)(param_1 + 0x251);
  }
  else {
    uStack_38 = *(undefined8 *)(param_1 + 0xf8);
    local_40 = *(ulong *)(param_1 + 0xf0);
    local_30 = *(ulong *)(param_1 + 0x100);
    cVar1 = *(char *)(param_1 + 0x251);
  }
  if (cVar1 == '\0') {
    bVar2 = false;
    cVar1 = *(char *)(param_1 + 0x128);
  }
  else {
    bVar2 = *(char *)(param_1 + 0x250) != '\0';
    cVar1 = *(char *)(param_1 + 0x128);
  }
  if (cVar1 == '\0') {
    local_50 = local_50 & 0xffffffffffffff;
    local_60 = local_60 & 0xffffffffffffff00;
  }
  else if (*(char *)(param_1 + 0x127) < '\0') {
    func_0x000101e45618(&local_60,*(undefined8 *)(param_1 + 0x110),*(undefined8 *)(param_1 + 0x118))
    ;
  }
  else {
    uStack_58 = *(undefined8 *)(param_1 + 0x118);
    local_60 = *(ulong *)(param_1 + 0x110);
    local_50 = *(ulong *)(param_1 + 0x120);
  }
  uVar3 = func_0x0001017c15ac(&local_60);
  uVar3 = func_0x0001018b0a8c(&local_40,bVar2,uVar3);
  if ((long)local_50 < 0) {
    func_0x000103462518(local_60);
  }
  if (-1 < (long)local_30) {
    return uVar3;
  }
  func_0x000103462518(local_40);
  return uVar3;
}

