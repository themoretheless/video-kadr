
uint entry(double param_1,long param_2)

{
  double *pdVar1;
  ulong uVar2;
  uint uVar3;
  long *plVar4;
  long lVar5;
  int iVar6;
  int iVar7;
  int iVar8;
  int iVar9;
  ulong uVar10;
  double local_50;
  double dStack_48;
  double local_40;
  double local_38;
  
  plVar4 = *(long **)(param_2 + 8);
  if (((plVar4 != (long *)0x0) && (*(long *)(param_2 + 0x1e0) != 0)) && ((int)plVar4[0x35] == 0xc1))
  {
    lVar5 = *(long *)(param_2 + 0x298);
    uVar2 = *(long *)(param_2 + 0x2a0) - lVar5;
    if (uVar2 != 0) {
      iVar7 = (int)(uVar2 >> 3) + -1;
      if (iVar7 < 2) {
        iVar8 = 0;
      }
      else {
        iVar8 = 0;
        iVar6 = iVar7;
        iVar9 = iVar7;
        do {
          if (iVar9 < 0) {
            iVar9 = iVar9 + 1;
          }
          iVar9 = iVar9 >> 1;
          iVar7 = iVar9;
          if (*(double *)(lVar5 + (long)iVar9 * 8) <= param_1) {
            iVar7 = iVar6;
            iVar8 = iVar9;
          }
          iVar9 = iVar8 + iVar7;
          iVar6 = iVar7;
        } while (iVar8 + 1 < iVar7);
      }
      if (ABS(*(double *)(lVar5 + (long)iVar7 * 8) - param_1) <=
          ABS(*(double *)(lVar5 + (long)iVar8 * 8) - param_1)) {
        iVar8 = iVar7;
      }
      uVar10 = (ulong)iVar8;
      if (((uVar10 < (ulong)((long)uVar2 >> 3)) &&
          (uVar10 < (ulong)(*(long *)(param_2 + 0x3c8) - *(long *)(param_2 + 0x3c0) >> 3))) &&
         ((uVar10 < (ulong)(*(long *)(param_2 + 0x270) - *(long *)(param_2 + 0x268) >> 5) &&
          (uVar10 < (ulong)(*(long *)(param_2 + 600) - *(long *)(param_2 + 0x250) >> 5))))) {
        (**(code **)(*plVar4 + 0x20))(*(undefined8 *)(*(long *)(param_2 + 0x3c0) + uVar10 * 8));
        pdVar1 = (double *)(*(long *)(param_2 + 0x268) + uVar10 * 0x20);
        local_40 = -pdVar1[2];
        local_50 = -*pdVar1;
        dStack_48 = -pdVar1[1];
        local_38 = pdVar1[3];
        uVar3 = func_0x000102816900(*(undefined8 *)(*(long *)(param_2 + 0x298) + uVar10 * 8),
                                    *(undefined8 *)(param_2 + 0x90),*(undefined8 *)(param_2 + 8),
                                    &local_50,*(long *)(param_2 + 0x250) + uVar10 * 0x20,
                                    *(undefined1 *)(param_2 + 0x98),param_2 + 0x1c8);
        return uVar3 ^ 1;
      }
    }
  }
  return 1;
}

