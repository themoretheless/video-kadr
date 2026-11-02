
void entry(undefined8 param_1,undefined8 *param_2,undefined8 *param_3)

{
  int iVar1;
  long lVar2;
  undefined8 *puVar3;
  double *pdVar4;
  long lVar5;
  int iVar6;
  int iVar7;
  double dVar8;
  undefined1 auVar9 [16];
  double dVar10;
  double dVar11;
  undefined1 auVar12 [16];
  double dVar13;
  long local_40;
  long local_38;
  
  local_40 = 0;
  local_38 = 0;
  func_0x0001027a4bac(param_1,&local_40);
  if (0 < local_38) {
    pdVar4 = (double *)(local_40 + 0x30);
    lVar2 = local_38;
    do {
      iVar6 = (int)*param_3;
      auVar9._0_8_ = (long)iVar6;
      iVar7 = (int)((ulong)*param_3 >> 0x20);
      auVar9._8_8_ = (long)iVar7;
      auVar9 = NEON_scvtf(auVar9,8);
      auVar12._0_8_ = (long)SUB84(pdVar4[-4],0);
      auVar12._8_8_ = (long)(int)((ulong)pdVar4[-4] >> 0x20);
      auVar12 = NEON_scvtf(auVar12,8);
      dVar8 = auVar9._0_8_ / auVar12._0_8_;
      dVar10 = auVar9._8_8_ / auVar12._8_8_;
      dVar11 = dVar8 * *pdVar4;
      dVar13 = dVar10 * pdVar4[1];
      pdVar4[1] = dVar13;
      *pdVar4 = dVar11;
      pdVar4[-2] = dVar10 * pdVar4[-2];
      pdVar4[-3] = dVar8 * pdVar4[-3];
      iVar1 = *(int *)(param_3 + 1);
      *pdVar4 = dVar11 + ((double)param_3[2] - (double)(iVar6 - iVar1) * 0.5);
      pdVar4[1] = dVar13 + ((double)param_3[3] -
                           (double)(iVar7 - *(int *)((long)param_3 + 0xc)) * 0.5);
      *(int *)(pdVar4 + -4) = iVar1;
      *(undefined4 *)((long)pdVar4 + -0x1c) = *(undefined4 *)((long)param_3 + 0xc);
      pdVar4 = pdVar4 + 0x18;
      lVar2 = lVar2 + -1;
    } while (lVar2 != 0);
  }
  if (*(char *)((long)param_2 + 0x17) < '\0') {
    *(undefined1 *)*param_2 = 0;
    param_2[1] = 0;
  }
  else {
    *(undefined1 *)param_2 = 0;
    *(undefined1 *)((long)param_2 + 0x17) = 0;
  }
  func_0x0001027a8830(&local_40,param_2);
  lVar2 = local_40;
  if ((local_40 != 0) && (local_38 != 0)) {
    puVar3 = (undefined8 *)(local_40 + local_38 * 0xc0);
    lVar5 = local_38;
    do {
      puVar3 = puVar3 + -0x18;
      lVar5 = lVar5 + -1;
      func_0x000103462eb4(*puVar3);
    } while (lVar5 != 0);
  }
  func_0x000103462eb4(lVar2);
  return;
}

