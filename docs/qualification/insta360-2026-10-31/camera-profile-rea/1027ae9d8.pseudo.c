/* REA/Ghidra function-only observation; not original source or product code. */

undefined8 entry(double *param_1,double *param_2,double *param_3,double *param_4)

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
  double dVar11;
  double dVar12;
  double dVar13;
  double dVar14;
  double dVar15;
  double dVar16;
  double dVar17;
  double dVar18;
  double dVar19;
  
  dVar8 = *param_2;
  dVar1 = param_2[1];
  dVar9 = dVar8 * dVar8;
  dVar3 = dVar1 * dVar1;
  dVar10 = dVar8 * dVar1;
  dVar2 = dVar9 + dVar3;
  dVar7 = dVar2 * dVar2;
  dVar5 = dVar2 * dVar7;
  dVar4 = dVar2 * dVar5;
  dVar6 = dVar2 * dVar4;
  dVar11 = dVar2 * *param_1 + 1.0 + dVar7 * param_1[1] + dVar5 * param_1[2] + dVar4 * param_1[3] +
           dVar6 * param_1[4];
  dVar12 = dVar2 + dVar9 * 2.0;
  dVar13 = param_1[6] + param_1[8] * dVar2;
  *param_3 = dVar12 * (param_1[5] + param_1[7] * dVar2) + dVar11 * dVar8 +
             dVar10 * (dVar13 + dVar13) + dVar2 * param_1[9] + dVar7 * param_1[0xb];
  dVar13 = dVar2 + dVar3 * 2.0;
  dVar14 = param_1[5] + param_1[7] * dVar2;
  param_3[1] = dVar13 * (param_1[6] + param_1[8] * dVar2) + dVar11 * param_2[1] +
               dVar10 * (dVar14 + dVar14) + dVar2 * param_1[10] + dVar7 * param_1[0xc];
  dVar11 = param_1[7];
  dVar16 = param_1[5] + dVar11 * dVar2;
  dVar17 = param_1[8];
  dVar14 = param_1[6] + dVar17 * dVar2;
  *param_4 = dVar8 * 6.0 * dVar16 + dVar12 * dVar8 * (dVar11 + dVar11) +
             dVar1 * dVar9 * dVar17 * 4.0 + dVar1 * (dVar14 + dVar14) +
             *param_1 * (dVar3 + dVar9 * 3.0) + 1.0 + param_1[1] * (dVar7 + dVar2 * dVar9 * 4.0) +
             param_1[2] * (dVar5 + dVar7 * dVar9 * 6.0) + param_1[3] * (dVar4 + dVar5 * dVar9 * 8.0)
             + param_1[4] * (dVar6 + dVar4 * dVar9 * 10.0) + dVar8 * (param_1[9] + param_1[9]) +
             dVar2 * dVar8 * param_1[0xb] * 4.0;
  dVar15 = dVar10 * 4.0 * dVar2;
  dVar18 = dVar10 * 6.0 * dVar7;
  dVar19 = dVar10 * 8.0 * dVar5;
  param_4[2] = (dVar1 + dVar1) * dVar16 + dVar12 * dVar1 * (dVar11 + dVar11) +
               dVar8 * dVar3 * dVar17 * 4.0 + dVar8 * (dVar14 + dVar14) +
               dVar15 * param_1[1] + *param_1 * (dVar10 + dVar10) + param_1[2] * dVar18 +
               param_1[3] * dVar19 + dVar4 * dVar10 * param_1[4] * 10.0 +
               dVar1 * (param_1[9] + param_1[9]) + dVar2 * dVar1 * param_1[0xb] * 4.0;
  param_4[1] = (dVar8 + dVar8) * dVar14 + dVar13 * dVar8 * (dVar17 + dVar17) +
               dVar1 * dVar9 * dVar11 * 4.0 + dVar1 * (dVar16 + dVar16) +
               dVar15 * param_1[1] + *param_1 * (dVar10 + dVar10) + param_1[2] * dVar18 +
               param_1[3] * dVar19 + dVar4 * dVar10 * param_1[4] * 10.0 +
               dVar8 * (param_1[10] + param_1[10]) + dVar2 * dVar8 * param_1[0xc] * 4.0;
  param_4[3] = dVar1 * 6.0 * dVar14 + dVar13 * dVar1 * (dVar17 + dVar17) +
               dVar8 * dVar3 * dVar11 * 4.0 + dVar8 * (dVar16 + dVar16) +
               *param_1 * (dVar9 + dVar3 * 3.0) + 1.0 + param_1[1] * (dVar7 + dVar2 * dVar3 * 4.0) +
               param_1[2] * (dVar5 + dVar7 * dVar3 * 6.0) +
               param_1[3] * (dVar4 + dVar5 * dVar3 * 8.0) +
               param_1[4] * (dVar6 + dVar4 * dVar3 * 10.0) + dVar1 * (param_1[10] + param_1[10]) +
               dVar2 * dVar1 * param_1[0xc] * 4.0;
  return 1;
}

