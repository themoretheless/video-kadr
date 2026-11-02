
void entry(long param_1,double *param_2,double *param_3,double *param_4)

{
  double dVar1;
  double dVar2;
  double dVar3;
  double dVar4;
  double dVar5;
  double dVar6;
  double dVar7;
  double dVar8;
  double dVar9;
  double dVar10;
  double local_70;
  double dStack_68;
  double local_60;
  
  dVar2 = 6.283185307179586 - *(double *)(param_1 + 0x18);
  dVar9 = dVar2 - *(double *)(param_1 + 0x20) * *param_2;
  dVar1 = (double)func_0x0001034627b8(1.5707963267948966 - param_2[1] * *(double *)(param_1 + 0x28))
  ;
  dVar3 = dVar2;
  dVar9 = (double)func_0x0001034627b8(dVar9);
  dVar3 = dVar2 * dVar3;
  dVar2 = dVar2 * dVar9;
  dVar5 = *param_3;
  dVar4 = param_3[1];
  dVar7 = param_3[2];
  dVar8 = param_3[3];
  dVar6 = -(dVar5 * dVar1) + dVar3 * dVar7;
  dVar10 = -(dVar7 * dVar2) + dVar1 * dVar4;
  dVar9 = -(dVar4 * dVar3) + dVar2 * dVar5;
  dVar10 = dVar10 + dVar10;
  dVar6 = dVar6 + dVar6;
  dVar9 = dVar9 + dVar9;
  local_70 = dVar3 + dVar10 * dVar8 + -dVar7 * dVar6 + dVar9 * dVar4;
  dStack_68 = dVar2 + dVar6 * dVar8 + -(dVar5 * dVar9) + dVar7 * dVar10;
  local_60 = dVar1 + dVar8 * dVar9 + -dVar4 * dVar10 + dVar5 * dVar6;
  (**(code **)(**(long **)(param_1 + 8) + 0x28))(*(long **)(param_1 + 8),&local_70,param_4);
  *param_4 = *(double *)(param_1 + 0x40) * *param_4;
  param_4[1] = *(double *)(param_1 + 0x48) * param_4[1];
  return;
}

