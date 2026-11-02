
undefined1  [16] entry(double param_1,long param_2)

{
  long lVar1;
  ulong uVar2;
  double *pdVar3;
  ulong uVar4;
  long *plVar5;
  ulong uVar6;
  ulong uVar7;
  undefined1 auVar8 [16];
  double local_70;
  double dStack_68;
  double local_60;
  double local_58;
  
  plVar5 = *(long **)(param_2 + 0x30);
  uVar2 = (**(code **)(*plVar5 + 0x58))(plVar5);
  pdVar3 = (double *)func_0x000103462620(0x10);
  *pdVar3 = 0.0;
  pdVar3[1] = 0.0;
  uVar6 = 0;
  if (uVar2 != 0) {
    do {
      uVar7 = uVar2 + uVar6 >> 1;
      uVar4 = (**(code **)(*plVar5 + 0x10))(plVar5,pdVar3,uVar7);
      if ((uVar4 & 1) == 0) {
        uVar6 = (**(code **)(*plVar5 + 0x58))(plVar5);
        break;
      }
      if (*pdVar3 < param_1) {
        uVar6 = uVar7 + 1;
        uVar7 = uVar2;
      }
      uVar2 = uVar7;
    } while (uVar6 < uVar2);
  }
  func_0x000103462518(pdVar3);
  uVar2 = (**(code **)(**(long **)(param_2 + 0x30) + 0x58))();
  lVar1 = uVar6 - (uVar6 == uVar2);
  if (lVar1 == 0) {
    (**(code **)(**(long **)(param_2 + 0x30) + 0x10))(*(long **)(param_2 + 0x30),&local_60,0);
  }
  else {
    (**(code **)(**(long **)(param_2 + 0x30) + 0x10))
              (*(long **)(param_2 + 0x30),&local_60,lVar1 + -1);
    (**(code **)(**(long **)(param_2 + 0x30) + 0x10))(*(long **)(param_2 + 0x30),&local_70,lVar1);
    local_58 = local_58 + (dStack_68 - local_58) * ((param_1 - local_60) / (local_70 - local_60));
  }
  auVar8._8_8_ = 0;
  auVar8._0_8_ = local_58;
  return auVar8;
}

