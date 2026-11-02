/* REA/Ghidra pseudocode observation; not original source or product implementation. */

void __ZN9arvrender23PrecomputeStabilization19ConverToInsGyroDataEPKhmRNSt3__16vectorIN3ins10Stabilizer8GyroDataENS3_9allocatorIS7_EEEEi
               (long param_1,long param_2,ulong param_3,undefined8 param_4,int param_5)

{
  char cVar1;
  undefined2 uVar2;
  undefined2 uVar3;
  undefined2 uVar4;
  undefined2 uVar5;
  undefined2 uVar6;
  bool bVar7;
  bool bVar8;
  undefined2 *puVar9;
  long lVar10;
  double *pdVar11;
  ulong uVar12;
  ulong uVar13;
  double dVar14;
  double dVar15;
  double dVar16;
  double local_b0;
  double dStack_a8;
  double local_a0;
  double dStack_98;
  double dStack_90;
  double local_88;
  undefined8 local_80;
  undefined8 uStack_78;
  undefined8 uStack_70;
  double local_68;
  
  if (param_3 != 0) {
    lVar10 = (long)param_5;
    if (*(char *)(param_1 + 0x251) == '\0') {
      uVar13 = 0;
      pdVar11 = (double *)(param_2 + 0x18);
      do {
        dVar14 = pdVar11[-3];
        dStack_a8 = pdVar11[-1];
        local_b0 = pdVar11[-2];
        local_a0 = *pdVar11;
        dStack_90 = pdVar11[2];
        dStack_98 = pdVar11[1];
        local_88 = pdVar11[3];
        cVar1 = *(char *)(param_1 + 0x3f8);
        if ((cVar1 != '\0') || (*(long *)(param_1 + 0x340) < (long)dVar14)) {
          uStack_78 = 0;
          local_80 = 0;
          uStack_70 = 0;
          local_68 = (double)(long)dVar14 / *(double *)(param_1 + 0x288);
          dVar15 = (double)NEON_ucvtf((ulong)*(uint *)(param_1 + 0x278));
          bVar7 = false;
          if ((ABS(local_b0) < dVar15) && (bVar7 = false, !NAN(ABS(dStack_a8)) && !NAN(dVar15))) {
            bVar7 = ABS(dStack_a8) < dVar15;
          }
          bVar8 = false;
          if ((bVar7) && (bVar8 = false, !NAN(ABS(local_a0)) && !NAN(dVar15))) {
            bVar8 = ABS(local_a0) < dVar15;
          }
          if (bVar8) {
            dVar15 = (double)NEON_ucvtf((ulong)*(uint *)(param_1 + 0x27c));
            dVar15 = dVar15 * 0.017453292519943295;
            if ((dVar15 <= ABS(dStack_98)) || (dVar15 <= ABS(dStack_90))) goto LAB_101843a7c;
            if ((dVar15 <= ABS(local_88)) && (0.0 <= local_68 || cVar1 == '\0')) goto LAB_1018439d8;
          }
          else {
LAB_101843a7c:
            if ((cVar1 == '\0') || (0.0 <= local_68)) {
LAB_1018439d8:
              if (*(long *)(param_1 + 0x210) == 0) goto LAB_1018439f0;
            }
          }
          *(double *)(param_1 + 0x340) = dVar14;
          FUN_1003d554c(param_4,&local_b0);
        }
LAB_1018439f0:
        pdVar11 = pdVar11 + lVar10 * 7;
        uVar13 = uVar13 + lVar10;
      } while (uVar13 < param_3);
    }
    else {
      uVar13 = 0;
      puVar9 = (undefined2 *)(param_2 + 10);
      do {
        uVar12 = *(ulong *)(puVar9 + -5);
        uVar2 = *puVar9;
        uVar3 = puVar9[1];
        uVar4 = puVar9[2];
        uVar5 = puVar9[3];
        uVar6 = puVar9[4];
        if ((*(char *)(param_1 + 0x3f8) != '\0') || (*(ulong *)(param_1 + 0x348) < uVar12)) {
          uStack_78 = 0;
          local_80 = 0;
          local_68 = 0.0;
          uStack_70 = 0;
          dStack_98 = 0.0;
          local_a0 = 0.0;
          local_88 = 0.0;
          dStack_90 = 0.0;
          dStack_a8 = 0.0;
          local_b0 = 0.0;
          dVar14 = (double)__ZN9arvrender14StabilizerUtil6CvtAccEtj
                                     (puVar9[-1],*(undefined4 *)(param_1 + 0x278));
          dVar15 = (double)__ZN9arvrender14StabilizerUtil6CvtAccEtj
                                     (uVar2,*(undefined4 *)(param_1 + 0x278));
          local_a0 = (double)__ZN9arvrender14StabilizerUtil6CvtAccEtj
                                       (uVar3,*(undefined4 *)(param_1 + 0x278));
          local_b0 = dVar14;
          dStack_a8 = dVar15;
          dVar14 = (double)__ZN9arvrender14StabilizerUtil7CvtGyroEtj
                                     (uVar4,*(undefined4 *)(param_1 + 0x27c));
          dVar15 = (double)__ZN9arvrender14StabilizerUtil7CvtGyroEtj
                                     (uVar5,*(undefined4 *)(param_1 + 0x27c));
          local_88 = (double)__ZN9arvrender14StabilizerUtil7CvtGyroEtj
                                       (uVar6,*(undefined4 *)(param_1 + 0x27c));
          local_68 = (double)uVar12 / 1000000.0;
          dVar16 = (double)NEON_ucvtf((ulong)*(uint *)(param_1 + 0x278));
          if ((dVar16 <= ABS(local_b0)) || ((dVar16 <= ABS(dStack_a8) || (dVar16 <= ABS(local_a0))))
             ) {
LAB_10184382c:
            if (*(long *)(param_1 + 0x210) == 0) goto LAB_101843844;
          }
          else {
            dVar16 = (double)NEON_ucvtf((ulong)*(uint *)(param_1 + 0x27c));
            dVar16 = dVar16 * 0.017453292519943295;
            if ((dVar16 <= ABS(dVar14)) || ((dVar16 <= ABS(dVar15) || (dVar16 <= ABS(local_88)))))
            goto LAB_10184382c;
          }
          *(ulong *)(param_1 + 0x348) = uVar12;
          dStack_98 = dVar14;
          dStack_90 = dVar15;
          FUN_1003d554c(param_4,&local_b0);
        }
LAB_101843844:
        puVar9 = puVar9 + lVar10 * 10;
        uVar13 = uVar13 + lVar10;
      } while (uVar13 < param_3);
    }
  }
  return;
}

