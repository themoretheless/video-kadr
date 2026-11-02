/* REA/Ghidra function-only observation; not original source or product code. */

/* WARNING: Control flow encountered bad instruction data */

void entry(undefined8 *param_1,long param_2)

{
  long lVar1;
  long *plVar2;
  undefined8 *puVar3;
  long *plVar4;
  long lVar5;
  long lVar6;
  long lVar7;
  long lVar8;
  long lVar9;
  
  plVar4 = *(long **)(param_2 + 8);
  plVar2 = (long *)func_0x000103462620(0xf0);
  plVar2[2] = 0;
  *plVar2 = 0x104176308;
  plVar2[1] = 0;
  lVar1 = *plVar4;
  plVar2[5] = plVar4[1];
  plVar2[4] = lVar1;
  lVar9 = plVar4[0xb];
  lVar8 = plVar4[10];
  lVar5 = plVar4[0xd];
  lVar1 = plVar4[0xc];
  lVar7 = plVar4[9];
  lVar6 = plVar4[8];
  plVar2[0x12] = plVar4[0xe];
  plVar2[0xd] = lVar7;
  plVar2[0xc] = lVar6;
  plVar2[0xf] = lVar9;
  plVar2[0xe] = lVar8;
  plVar2[0x11] = lVar5;
  plVar2[0x10] = lVar1;
  lVar5 = plVar4[3];
  lVar1 = plVar4[2];
  lVar6 = plVar4[4];
  lVar8 = plVar4[7];
  lVar7 = plVar4[6];
  plVar2[9] = plVar4[5];
  plVar2[8] = lVar6;
  plVar2[0xb] = lVar8;
  plVar2[10] = lVar7;
  plVar2[7] = lVar5;
  plVar2[6] = lVar1;
  lVar1 = plVar4[0x10];
  plVar2[0x15] = plVar4[0x11];
  plVar2[0x14] = lVar1;
  lVar1 = plVar4[0x12];
  plVar2[0x17] = plVar4[0x13];
  plVar2[0x16] = lVar1;
  lVar1 = plVar4[0x14];
  plVar2[0x19] = plVar4[0x15];
  plVar2[0x18] = lVar1;
  lVar1 = plVar4[0x16];
  plVar2[0x1b] = plVar4[0x17];
  plVar2[0x1a] = lVar1;
  plVar2[0x1c] = plVar4[0x18];
  puVar3 = (undefined8 *)func_0x000103462620(0x30);
  puVar3[2] = 0;
  *puVar3 = 0x104176228;
  puVar3[1] = 0;
  plVar4 = plVar2 + 1;
  *plVar4 = *plVar4 + 1;
  puVar3[3] = 0x104176278;
  puVar3[4] = plVar2 + 4;
  puVar3[5] = plVar2;
  *param_1 = puVar3 + 3;
  param_1[1] = puVar3;
  LOAcquire();
  lVar1 = *plVar4;
  *plVar4 = lVar1 + -1;
  LORelease();
  if (lVar1 != 0) {
    return;
  }
  (**(code **)(*plVar2 + 0x10))(plVar2);
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}

