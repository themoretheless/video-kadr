/* REA/Ghidra pseudocode observation; not original source or product implementation. */

undefined8
__ZN10extra_info14GyroDataParser15ParseFromBufferEOKNSt3__16vectorIhNS1_9allocatorIhEEEE
          (long param_1,long *param_2)

{
  ulong uVar1;
  undefined8 uVar2;
  ulong uVar3;
  ulong uVar4;
  
  if (*(long **)(param_1 + 0x40) == (long *)0x0) {
    if ((long *)(param_1 + 8) != param_2) {
      FUN_1001b943c((long *)(param_1 + 8),*param_2,param_2[1],param_2[1] - *param_2);
    }
    uVar3 = *(long *)(param_1 + 0x10) - (long)*(undefined8 **)(param_1 + 8);
    uVar4 = *(ulong *)(param_1 + 0x60);
    uVar1 = 0;
    if (uVar4 != 0) {
      uVar1 = uVar3 / uVar4;
    }
    *(ulong *)(param_1 + 0x20) = uVar1;
    uVar2 = 1;
    if ((*(long *)(param_1 + 0x28) == 0) && (uVar4 <= uVar3)) {
      *(undefined8 *)(param_1 + 0x28) = **(undefined8 **)(param_1 + 8);
      return uVar2;
    }
  }
  else {
    uVar2 = (**(code **)(**(long **)(param_1 + 0x40) + 0x10))();
    if ((int)uVar2 != 0) {
      uVar1 = 0;
      if (*(ulong *)(param_1 + 0x60) != 0) {
        uVar1 = (ulong)(*(long *)(param_1 + 0x10) - *(long *)(param_1 + 8)) /
                *(ulong *)(param_1 + 0x60);
      }
      *(ulong *)(param_1 + 0x20) = uVar1;
      uVar2 = 1;
    }
  }
  return uVar2;
}

