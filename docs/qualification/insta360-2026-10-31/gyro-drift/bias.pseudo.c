
void entry(undefined8 *param_1,long param_2)

{
  undefined8 uVar1;
  
  uVar1 = *(undefined8 *)(param_2 + 0x60);
  param_1[1] = *(undefined8 *)(param_2 + 0x68);
  *param_1 = uVar1;
  param_1[2] = *(undefined8 *)(param_2 + 0x70);
  return;
}

