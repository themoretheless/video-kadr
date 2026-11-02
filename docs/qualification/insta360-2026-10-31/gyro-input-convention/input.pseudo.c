
undefined8 entry(long param_1,double *param_2,ulong param_3)

{
  long lVar1;
  double dVar2;
  double dVar3;
  double dVar4;
  double dVar5;
  double dVar6;
  double dVar7;
  double dVar8;
  int iVar9;
  undefined8 uVar10;
  double dVar11;
  double dVar12;
  double local_120;
  double dStack_118;
  double local_110;
  double local_108;
  double dStack_100;
  double local_f8;
  double local_f0;
  double dStack_e8;
  double local_e0;
  double dStack_d8;
  double local_d0;
  double dStack_c8;
  double local_c0;
  double local_b0;
  double dStack_a8;
  double local_a0;
  double local_98;
  double dStack_90;
  double local_88;
  double local_80;
  double dStack_78;
  double local_70;
  double local_68;
  
  dVar11 = param_2[3];
  if ((60.6217782649107 <= SQRT(param_2[5] * param_2[5] + dVar11 * dVar11 + param_2[4] * param_2[4])
      ) || (34.64101615137754 <=
            SQRT(param_2[2] * param_2[2] + *param_2 * *param_2 + param_2[1] * param_2[1]))) {
    return 0xffffffff;
  }
  lVar1 = param_1 + 0x2f8;
  func_0x000103462230(lVar1);
  if (*(long *)(param_1 + 0x10) == 0) {
    func_0x00010346223c(lVar1);
    return 0xffffffff;
  }
  dStack_a8 = param_2[1];
  local_b0 = *param_2;
  local_a0 = param_2[2];
  dStack_90 = param_2[4];
  local_98 = param_2[3];
  local_88 = param_2[5];
  dStack_78 = param_2[7];
  local_80 = param_2[6];
  local_70 = param_2[8];
  local_68 = param_2[9];
  uVar10 = 0;
  if (*(long *)(param_1 + 0x368) != 0) {
    func_0x0001027ca60c(*(long *)(param_1 + 0x368),&local_98);
    uVar10 = *(undefined8 *)(param_1 + 0x368);
  }
  func_0x0001027cabb8(&local_d0,uVar10);
  local_98 = local_98 - local_d0;
  dStack_90 = dStack_90 - dStack_c8;
  local_88 = local_88 - local_c0;
  *(int *)(param_1 + 0x50) = *(int *)(param_1 + 0x50) + 1;
  iVar9 = func_0x0001027df80c(*(undefined8 *)(param_1 + 0x30),&local_b0,&local_b0);
  if (iVar9 == 0) {
    uVar10 = 1;
  }
  else {
    if (*(char *)(param_1 + 0x54) != '\0') {
      func_0x0001027d6d98(&local_120,*(undefined8 *)(param_1 + 0x20),&local_b0);
      dVar8 = dStack_90;
      dVar7 = local_98;
      dVar6 = dStack_a8;
      dVar5 = local_b0;
      dVar4 = local_e0;
      dVar3 = dStack_e8;
      dVar2 = local_f0;
      dVar12 = local_f8;
      dVar11 = local_110;
      dStack_a8 = dStack_118;
      local_b0 = local_120;
      dStack_118 = dVar6;
      local_120 = dVar5;
      local_110 = local_a0;
      dStack_90 = dStack_100;
      local_98 = local_108;
      dStack_100 = dVar8;
      local_108 = dVar7;
      local_f8 = local_88;
      local_a0 = dVar11;
      local_88 = dVar12;
      dStack_e8 = dStack_78;
      local_f0 = local_80;
      local_e0 = local_70;
      dStack_78 = dVar3;
      local_80 = dVar2;
      local_68 = dStack_d8;
      local_70 = dVar4;
      if ((ulong)*(uint *)(param_1 + 0x50) <
          (ulong)((*(long **)(param_1 + 0x20))[1] - **(long **)(param_1 + 0x20) >> 3)) {
        uVar10 = 0xffffffff;
        goto LAB_1027bfd6c;
      }
    }
    if ((param_3 & 1) == 0) {
      func_0x0001027b0f4c(&local_b0,*(undefined4 *)(param_1 + 0x58));
    }
    uVar10 = (**(code **)(**(long **)(param_1 + 0x10) + 0x30))(*(long **)(param_1 + 0x10),&local_b0)
    ;
    func_0x000101032fdc(&local_120,*(long *)(param_1 + 0x10) + 0x50);
    dVar11 = local_68;
    dVar12 = (double)(**(code **)(**(long **)(param_1 + 0x10) + 0x50))();
    func_0x0001027b7bb4(dVar11 + dVar12,param_1,&local_120);
    *(double *)(param_1 + 0x68) = dVar11 + dVar12;
  }
LAB_1027bfd6c:
  func_0x00010346223c(lVar1);
  return uVar10;
}

