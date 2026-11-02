/* REA/Ghidra pseudocode observation; not original source or product implementation. */

undefined8
__ZN10extra_info14GyroDataParser8ReadNextEPNS_11RawGyroItemE(long param_1,undefined8 *param_2)

{
  undefined8 *puVar1;
  undefined8 uVar2;
  undefined8 uVar3;
  
  if (*(long *)(param_1 + 0x60) == 0x14) {
    if ((long)*(int *)(param_1 + 0x34) * 0x14 + 0x14U <=
        (ulong)(*(long *)(param_1 + 0x10) - *(long *)(param_1 + 8))) {
      puVar1 = (undefined8 *)(*(long *)(param_1 + 8) + (long)*(int *)(param_1 + 0x34) * 0x14);
      uVar3 = puVar1[1];
      uVar2 = *puVar1;
      *(undefined4 *)(param_2 + 2) = *(undefined4 *)(puVar1 + 2);
      param_2[1] = uVar3;
      *param_2 = uVar2;
      *(int *)(param_1 + 0x34) = *(int *)(param_1 + 0x34) + 1;
      *(undefined8 *)(param_1 + 0x38) = *param_2;
      return 1;
    }
  }
  return 0;
}

