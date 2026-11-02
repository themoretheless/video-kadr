/* REA/Ghidra function-only observation; not original source or product code. */

void entry(undefined8 *param_1,long param_2)

{
  undefined8 uVar1;
  
  uVar1 = *(undefined8 *)(*(long *)(param_2 + 8) + 0x80);
  param_1[1] = *(undefined8 *)(*(long *)(param_2 + 8) + 0x88);
  *param_1 = uVar1;
  return;
}

