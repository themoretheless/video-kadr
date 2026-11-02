
/* WARNING: Control flow encountered bad instruction data */

void entry(long param_1,int param_2,int param_3)

{
  double *pdVar1;
  int iVar2;
  double dVar3;
  double dVar4;
  undefined8 uVar5;
  int iVar6;
  long *plVar7;
  long lVar8;
  double *pdVar9;
  double *pdVar10;
  uint uVar11;
  long lVar12;
  long *plVar13;
  long lVar14;
  long lVar15;
  undefined4 uVar16;
  undefined4 uVar17;
  undefined4 uVar18;
  undefined4 uVar19;
  double dVar20;
  double dVar21;
  double dVar22;
  double dVar23;
  double dVar24;
  double dVar25;
  double dVar26;
  double dVar27;
  double dVar28;
  double dVar29;
  double dVar30;
  double dVar31;
  double dVar32;
  double local_f0;
  double dStack_e8;
  double local_e0;
  double dStack_d8;
  double local_d0;
  double dStack_c8;
  double local_c0;
  double dStack_b8;
  
  if ((param_3 < param_2) && (0 < *(long *)(**(long **)(param_1 + 0x50) + 0x10))) {
    iVar2 = *(int *)(param_1 + 0x5c);
    lVar12 = (long)(*(int *)(param_1 + 0x58) + iVar2 * param_3);
    do {
      plVar13 = *(long **)(param_1 + 0x50);
      if (0 < *(long *)(*plVar13 + 0x10)) {
        lVar14 = 0;
        lVar15 = plVar13[1];
LAB_101b51db0:
        do {
          local_d0 = (double)(int)lVar14;
          plVar7 = *(long **)(lVar15 + 0x38);
          dStack_c8 = (double)(int)lVar12;
          if (plVar7 == (long *)0x0) {
LAB_101b52230:
            func_0x000100023430();
                    /* WARNING: Bad instruction - Truncating control flow here */
            halt_baddata();
          }
          iVar6 = (**(code **)(*plVar7 + 0x30))(plVar7,&local_d0,plVar13[2],&local_c0);
          if (iVar6 != 0) {
            iVar6 = 5;
            do {
              dVar22 = dStack_b8;
              dVar30 = local_c0;
              iVar6 = iVar6 + -1;
              uVar16 = SUB84(local_c0,0);
              uVar18 = (undefined4)((ulong)local_c0 >> 0x20);
              if (iVar6 == 0) break;
              uVar17 = SUB84(dStack_b8,0);
              uVar19 = (int)((ulong)dStack_b8 >> 0x20);
              if (*(char *)plVar13[3] == '\0') {
                uVar17 = uVar16;
                uVar19 = uVar18;
              }
              pdVar9 = (double *)plVar13[5];
              dVar29 = (double)CONCAT44(uVar19,uVar17) *
                       *(double *)(plVar13[4] + (ulong)(*(char *)plVar13[3] != '\0') * 8) * 50.0;
              uVar11 = (uint)dVar29;
              if (uVar11 < 0x32) {
                dVar29 = dVar29 - (double)(int)uVar11;
                lVar8 = *(long *)(*(long *)plVar13[6] + 0xa8);
                pdVar10 = (double *)(lVar8 + (ulong)uVar11 * 0x20);
                pdVar1 = (double *)(lVar8 + (ulong)(uVar11 + 1) * 0x20);
                dVar20 = pdVar10[1];
                dVar23 = *pdVar10;
                dVar24 = pdVar10[3];
                dVar21 = pdVar10[2];
                dVar28 = pdVar1[1];
                dVar27 = *pdVar1;
                dVar26 = pdVar1[3];
                dVar25 = pdVar1[2];
                dVar31 = dVar23 * dVar27 + dVar21 * dVar25 + dVar20 * dVar28 + dVar24 * dVar26;
                if (0.9999999999999998 <= ABS(dVar31)) {
                  dVar32 = 1.0 - dVar29;
                }
                else {
                  dVar3 = (double)func_0x0001034628c0();
                  dVar4 = (double)func_0x000103463a54();
                  dVar32 = (double)func_0x000103463a54(SUB84((1.0 - dVar29) * dVar3,0));
                  dVar32 = dVar32 / dVar4;
                  dVar29 = (double)func_0x000103463a54(SUB84(dVar29 * dVar3,0));
                  dVar29 = dVar29 / dVar4;
                }
                dVar3 = -dVar29;
                if (0.0 <= dVar31) {
                  dVar3 = dVar29;
                }
                dVar29 = dVar23 * dVar32 + dVar27 * dVar3;
                dVar20 = dVar20 * dVar32 + dVar28 * dVar3;
                dVar23 = dVar21 * dVar32 + dVar25 * dVar3;
                dVar21 = dVar24 * dVar32 + dVar26 * dVar3;
              }
              else {
                pdVar10 = *(double **)(*(long *)plVar13[6] + 0xa8);
                if ((int)uVar11 < 0) {
                  dVar20 = pdVar10[1];
                  dVar29 = *pdVar10;
                  dVar21 = pdVar10[3];
                  dVar23 = pdVar10[2];
                }
                else {
                  dVar20 = pdVar10[0xc9];
                  dVar29 = pdVar10[200];
                  dVar21 = pdVar10[0xcb];
                  dVar23 = pdVar10[0xca];
                }
              }
              dVar24 = *pdVar9;
              dVar25 = pdVar9[1];
              dVar28 = pdVar9[2];
              dVar27 = pdVar9[3];
              dVar26 = ((-dVar24 * dVar29 + dVar27 * dVar21) - dVar25 * dVar20) - dVar28 * dVar23;
              dVar31 = (dVar24 * dVar21 + dVar27 * dVar29 + dVar25 * dVar23) - dVar28 * dVar20;
              dVar32 = (dVar25 * dVar21 + dVar27 * dVar20 + dVar28 * dVar29) - dVar24 * dVar23;
              dVar24 = (dVar28 * dVar21 + dVar27 * dVar23 + dVar24 * dVar20) - dVar25 * dVar29;
              pdVar9 = (double *)plVar13[7];
              dVar29 = *pdVar9;
              dVar23 = pdVar9[1];
              dVar21 = pdVar9[2];
              dVar20 = pdVar9[3];
              dStack_d8 = ((-(dVar31 * dVar29) + dVar20 * dVar26) - dVar23 * dVar32) -
                          dVar21 * dVar24;
              local_f0 = (dVar31 * dVar20 + dVar29 * dVar26 + dVar21 * dVar32) - dVar23 * dVar24;
              dStack_e8 = (dVar32 * dVar20 + dVar23 * dVar26 + dVar29 * dVar24) - dVar21 * dVar31;
              local_e0 = (dVar20 * dVar24 + dVar21 * dVar26 + dVar23 * dVar31) - dVar29 * dVar32;
              plVar7 = *(long **)(lVar15 + 0x38);
              if (plVar7 == (long *)0x0) goto LAB_101b52230;
              (**(code **)(*plVar7 + 0x30))(plVar7,&local_d0,&local_f0,&local_c0);
              uVar16 = SUB84(local_c0,0);
              uVar18 = (undefined4)((ulong)local_c0 >> 0x20);
              dVar30 = (local_c0 - dVar30) * *(double *)plVar13[8];
              dVar22 = (dStack_b8 - dVar22) * ((double *)plVar13[8])[1];
            } while (4.0 <= SQRT(dVar30 * dVar30 + dVar22 * dVar22));
            uVar17 = SUB84(dStack_b8,0);
            uVar19 = (int)((ulong)dStack_b8 >> 0x20);
            if (*(char *)plVar13[3] == '\0') {
              uVar17 = uVar16;
              uVar19 = uVar18;
            }
            pdVar9 = (double *)plVar13[5];
            dVar30 = (double)CONCAT44(uVar19,uVar17) *
                     *(double *)(plVar13[4] + (ulong)(*(char *)plVar13[3] != '\0') * 8) * 50.0;
            uVar11 = (uint)dVar30;
            if (uVar11 < 0x32) {
              dVar30 = dVar30 - (double)(int)uVar11;
              lVar8 = *(long *)(*(long *)plVar13[6] + 0xa8);
              pdVar10 = (double *)(lVar8 + (ulong)uVar11 * 0x20);
              pdVar1 = (double *)(lVar8 + (ulong)(uVar11 + 1) * 0x20);
              dVar29 = pdVar10[1];
              dVar22 = *pdVar10;
              dVar20 = pdVar10[3];
              dVar23 = pdVar10[2];
              dVar26 = pdVar1[1];
              dVar25 = *pdVar1;
              dVar24 = pdVar1[3];
              dVar21 = pdVar1[2];
              dVar27 = dVar22 * dVar25 + dVar23 * dVar21 + dVar29 * dVar26 + dVar20 * dVar24;
              if (0.9999999999999998 <= ABS(dVar27)) {
                dVar28 = 1.0 - dVar30;
              }
              else {
                dVar31 = (double)func_0x0001034628c0();
                dVar32 = (double)func_0x000103463a54();
                dVar28 = (double)func_0x000103463a54(SUB84((1.0 - dVar30) * dVar31,0));
                dVar28 = dVar28 / dVar32;
                dVar30 = (double)func_0x000103463a54(SUB84(dVar30 * dVar31,0));
                dVar30 = dVar30 / dVar32;
              }
              dVar31 = -dVar30;
              if (0.0 <= dVar27) {
                dVar31 = dVar30;
              }
              dVar30 = dVar22 * dVar28 + dVar25 * dVar31;
              dVar29 = dVar29 * dVar28 + dVar26 * dVar31;
              dVar22 = dVar23 * dVar28 + dVar21 * dVar31;
              dVar23 = dVar20 * dVar28 + dVar24 * dVar31;
            }
            else {
              pdVar10 = *(double **)(*(long *)plVar13[6] + 0xa8);
              if ((int)uVar11 < 0) {
                dVar29 = pdVar10[1];
                dVar30 = *pdVar10;
                dVar23 = pdVar10[3];
                dVar22 = pdVar10[2];
              }
              else {
                dVar29 = pdVar10[0xc9];
                dVar30 = pdVar10[200];
                dVar23 = pdVar10[0xcb];
                dVar22 = pdVar10[0xca];
              }
            }
            dVar20 = *pdVar9;
            dVar21 = pdVar9[1];
            dVar26 = pdVar9[2];
            dVar25 = pdVar9[3];
            dVar24 = ((-dVar20 * dVar30 + dVar25 * dVar23) - dVar21 * dVar29) - dVar26 * dVar22;
            dVar27 = (dVar20 * dVar23 + dVar25 * dVar30 + dVar21 * dVar22) - dVar26 * dVar29;
            dVar28 = (dVar21 * dVar23 + dVar25 * dVar29 + dVar26 * dVar30) - dVar20 * dVar22;
            dVar20 = (dVar26 * dVar23 + dVar25 * dVar22 + dVar20 * dVar29) - dVar21 * dVar30;
            pdVar9 = (double *)plVar13[7];
            dVar30 = *pdVar9;
            dVar22 = pdVar9[1];
            dVar23 = pdVar9[2];
            dVar29 = pdVar9[3];
            dStack_d8 = ((-(dVar27 * dVar30) + dVar29 * dVar24) - dVar22 * dVar28) - dVar23 * dVar20
            ;
            local_f0 = (dVar27 * dVar29 + dVar30 * dVar24 + dVar23 * dVar28) - dVar22 * dVar20;
            dStack_e8 = (dVar28 * dVar29 + dVar22 * dVar24 + dVar30 * dVar20) - dVar23 * dVar27;
            local_e0 = (dVar29 * dVar20 + dVar23 * dVar24 + dVar22 * dVar27) - dVar30 * dVar28;
            plVar7 = *(long **)(lVar15 + 0x38);
            if (plVar7 == (long *)0x0) goto LAB_101b52230;
            iVar6 = (**(code **)(*plVar7 + 0x30))(plVar7,&local_d0,&local_f0,&local_c0);
            if (iVar6 != 0) {
              plVar7 = (long *)*plVar13;
              *(ulong *)(*plVar7 + (lVar14 + plVar7[3] * lVar12) * 8) =
                   CONCAT44((float)dStack_b8 + (float)((double *)plVar13[9])[1],
                            (float)local_c0 + (float)*(double *)plVar13[9]);
              lVar14 = lVar14 + 1;
              if (plVar7[2] <= lVar14) break;
              goto LAB_101b51db0;
            }
          }
          plVar7 = (long *)*plVar13;
          uVar5 = NEON_fmov(0xbf800000,4);
          *(undefined8 *)(*plVar7 + (lVar14 + plVar7[3] * lVar12) * 8) = uVar5;
          lVar14 = lVar14 + 1;
        } while (lVar14 < plVar7[2]);
      }
      param_3 = param_3 + 1;
      lVar12 = lVar12 + iVar2;
    } while (param_3 != param_2);
  }
  return;
}

