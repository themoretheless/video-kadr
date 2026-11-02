/* REA/Ghidra function-only observation; not original source or product code. */

void entry(float *param_1)

{
  uint uVar1;
  uint uVar2;
  float fVar3;
  float fVar4;
  float fVar5;
  float fVar6;
  float fVar7;
  float fVar8;
  float fVar9;
  
  fVar3 = *param_1;
  fVar5 = param_1[5];
  fVar6 = param_1[10];
  fVar7 = (fVar3 - fVar5) - fVar6;
  fVar8 = (fVar5 - fVar3) - fVar6;
  fVar9 = (fVar6 - fVar3) - fVar5;
  fVar5 = fVar3 + fVar5 + fVar6;
  fVar3 = fVar7;
  if (fVar7 <= fVar5) {
    fVar3 = fVar5;
  }
  fVar4 = fVar8;
  if (fVar8 <= fVar3) {
    fVar4 = fVar3;
  }
  uVar2 = 2;
  if (fVar8 <= fVar3) {
    uVar2 = (uint)(fVar5 < fVar7);
  }
  fVar3 = fVar9;
  if (fVar9 <= fVar4) {
    fVar3 = fVar4;
  }
  uVar1 = 3;
  if (fVar9 <= fVar4) {
    uVar1 = uVar2;
  }
                    /* WARNING: Could not recover jumptable at 0x000101036bf4. Too many branches */
                    /* WARNING: Treating indirect jump as call */
  (*(code *)((ulong)*(byte *)((ulong)uVar1 + 0x103912279) * 4 + 0x101036bf8))
            (SQRT(fVar3 + 1.0) * 0.5,0x3e800000,param_1[1],fVar6,param_1[6],param_1[2],param_1[9],
             param_1[4]);
  return;
}

