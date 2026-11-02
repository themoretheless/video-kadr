/* REA/Ghidra function-only observation; not original source or product code. */

undefined8 entry(long param_1,double *param_2,double *param_3)

{
  bool bVar1;
  double *pdVar2;
  int iVar3;
  double dVar4;
  double dVar5;
  double dVar6;
  double dVar7;
  double dVar8;
  double dVar9;
  double dVar10;
  double dVar11;
  double local_80;
  double dStack_78;
  double dStack_70;
  double dStack_68;
  double local_60;
  double dStack_58;
  
  pdVar2 = *(double **)(param_1 + 8);
  dVar6 = (*param_2 - *pdVar2) * pdVar2[0x14] * pdVar2[0x18];
  dVar9 = (param_2[1] - pdVar2[1]) * pdVar2[0x15];
  iVar3 = 0x13;
  dVar8 = dVar6;
  dVar10 = dVar9;
  do {
    local_80 = dVar8;
    dStack_78 = dVar10;
    func_0x0001027ae9d8(pdVar2 + 2,&local_80,&local_80,&dStack_70);
    dVar4 = dStack_70 * dStack_70 + dStack_68 * dStack_68;
    dVar5 = dStack_70 * local_60 + dStack_68 * dStack_58;
    dVar7 = local_60 * local_60 + dStack_58 * dStack_58;
    dVar11 = 1.0 / (-(dVar5 * dVar5) + dVar7 * dVar4);
    dVar7 = dVar7 * dVar11;
    dVar5 = -dVar5 * dVar11;
    dVar4 = dVar4 * dVar11;
    local_80 = dVar6 - local_80;
    dStack_78 = dVar9 - dStack_78;
    dVar8 = dVar8 + (dVar7 * dStack_70 + dVar5 * local_60) * local_80 +
                    (dVar7 * dStack_68 + dVar5 * dStack_58) * dStack_78;
    dVar10 = dVar10 + (dVar5 * dStack_70 + dVar4 * local_60) * local_80 +
                      (dVar5 * dStack_68 + dVar4 * dStack_58) * dStack_78;
    bVar1 = iVar3 != 0;
    iVar3 = iVar3 + -1;
  } while (1e-15 <= local_80 * local_80 + dStack_78 * dStack_78 && bVar1);
  dVar6 = dVar10 * dVar10 + dVar8 * dVar8;
  dVar9 = pdVar2[0x16];
  dVar9 = (-1.0 / (dVar9 + SQRT(1.0 - (dVar9 * dVar9 + -1.0) * dVar6))) * (dVar6 + 1.0) * dVar9 +
          1.0;
  dVar6 = 1.0 / SQRT(dVar6 + dVar9 * dVar9);
  param_3[1] = dVar10 * dVar6;
  *param_3 = dVar8 * dVar6;
  param_3[2] = dVar9 * dVar6;
  return 1;
}

