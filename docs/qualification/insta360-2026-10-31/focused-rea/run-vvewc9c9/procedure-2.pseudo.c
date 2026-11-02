/* REA/Ghidra pseudocode observation; not original source or product implementation. */

void __ZN10extra_info14GyroDataParserC1Ebx(undefined8 *param_1,int param_2,undefined8 param_3)

{
  undefined8 uVar1;
  
  *param_1 = &PTR___ZN10extra_info14GyroDataParserD1Ev_1040b1358;
  param_1[2] = 0;
  param_1[1] = 0;
  param_1[4] = 0;
  param_1[3] = 0;
  param_1[5] = param_3;
  *(undefined4 *)(param_1 + 6) = 0xffffffff;
  *(undefined8 *)((long)param_1 + 0x34) = 0;
  *(undefined8 *)((long)param_1 + 0x44) = 0;
  *(undefined8 *)((long)param_1 + 0x3c) = 0;
  *(undefined4 *)((long)param_1 + 0x4c) = 0;
  param_1[10] = 0x7d000000008;
  *(char *)(param_1 + 0xb) = (char)param_2;
  uVar1 = 0x14;
  if (param_2 == 0) {
    uVar1 = 0x38;
  }
  param_1[0xc] = uVar1;
  return;
}

