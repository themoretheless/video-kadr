/* REA/Ghidra pseudocode observation; not original source or product implementation. */

/* WARNING: Removing unreachable block (ram,0x0001027a67d0) */
/* WARNING: Removing unreachable block (ram,0x0001027a6054) */
/* WARNING: Removing unreachable block (ram,0x0001027a6148) */
/* WARNING: Removing unreachable block (ram,0x0001027a4d5c) */
/* WARNING: Removing unreachable block (ram,0x0001027a4f98) */
/* WARNING: Removing unreachable block (ram,0x0001027a4e74) */
/* WARNING: Removing unreachable block (ram,0x0001027a4fb0) */
/* WARNING: Removing unreachable block (ram,0x0001027a5b2c) */
/* WARNING: Removing unreachable block (ram,0x0001027a7004) */
/* WARNING: Removing unreachable block (ram,0x0001027a771c) */
/* WARNING: Removing unreachable block (ram,0x0001027a5640) */
/* WARNING: Removing unreachable block (ram,0x0001027a5694) */
/* WARNING: Type propagation algorithm not settling */

undefined8
__ZN12Insta360Lens12OffsetParser5ParseERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEERN5Eigen6MatrixINS_12CameraParamsELin1ELi1ELi0ELin1ELi1EEE
          (long *param_1,long *param_2)

{
  long lVar1;
  long lVar2;
  undefined8 *puVar3;
  byte bVar4;
  char cVar5;
  long *plVar6;
  uint uVar7;
  int iVar8;
  uint uVar9;
  int iVar10;
  int iVar11;
  int iVar12;
  undefined8 *******pppppppuVar13;
  char *pcVar14;
  double *pdVar15;
  undefined8 *puVar16;
  char *pcVar17;
  ulong uVar18;
  double dVar19;
  undefined8 *puVar20;
  ulong uVar21;
  ulong uVar22;
  char *pcVar23;
  long *plVar24;
  long lVar25;
  uint *puVar26;
  long lVar27;
  undefined8 uVar28;
  long lVar29;
  void *pvVar30;
  long lVar31;
  uint uVar32;
  ulong uVar33;
  ulong uVar34;
  ulong uVar35;
  double dVar36;
  double dVar37;
  double dVar38;
  double dVar39;
  double dVar40;
  double dVar41;
  undefined1 auVar42 [16];
  undefined1 auVar43 [16];
  undefined1 auVar44 [16];
  undefined1 auVar45 [16];
  undefined1 auVar46 [16];
  undefined1 auVar47 [16];
  undefined1 auVar48 [16];
  undefined1 auVar49 [16];
  undefined1 auVar50 [16];
  undefined1 auVar51 [16];
  undefined1 auVar52 [16];
  undefined1 auVar53 [16];
  undefined1 auVar54 [16];
  undefined1 auVar55 [16];
  undefined1 auVar56 [16];
  double dVar59;
  undefined1 auVar57 [16];
  undefined1 auVar58 [16];
  double dVar60;
  double dVar61;
  double dVar62;
  undefined1 auVar63 [16];
  double dVar64;
  double dVar65;
  double dVar66;
  double dVar67;
  double dVar68;
  uint local_230;
  double local_210;
  double local_1f0;
  double local_1e0;
  double local_1d0;
  double local_1c0;
  double local_1b0;
  double local_1a0;
  double local_190;
  double local_180;
  double local_170;
  double local_160;
  undefined2 local_140;
  undefined6 uStack_13e;
  ulong uStack_138;
  byte local_129;
  char *local_128;
  char *local_120;
  char *local_118;
  undefined8 *******local_110;
  double dStack_108;
  undefined8 local_100;
  double local_f8;
  double dStack_f0;
  double local_e8;
  uint local_e0;
  int local_dc;
  int local_d8;
  double local_d0;
  double dStack_c8;
  double local_c0;
  long local_b0;
  
  local_b0 = *(long *)PTR____stack_chk_guard_103fed798;
  bVar4 = *(byte *)((long)param_1 + 0x17);
  uVar21 = (ulong)bVar4;
  uVar18 = param_1[1];
  uVar35 = uVar18;
  if (-1 < (char)bVar4) {
    uVar35 = uVar21;
  }
  if (uVar35 != 0) {
    uVar34 = 0;
    local_128 = (char *)0x0;
    local_120 = (char *)0x0;
    local_118 = (char *)0x0;
    local_129 = 1;
    local_140 = 0x5f;
    plVar24 = (long *)*param_1;
    plVar6 = plVar24;
    if (-1 < (char)bVar4) {
      plVar6 = param_1;
    }
LAB_1027a4c34:
    if (*(char *)((long)plVar6 + uVar34) != '_') goto code_r0x0001027a4c40;
    uVar33 = 0;
    if ((uVar35 != uVar34) && (uVar34 != 0xffffffffffffffff)) {
      uVar33 = 0;
LAB_1027a4c80:
      if ((uint)uVar21 >> 7 == 0) {
        uVar18 = uVar21;
        plVar24 = param_1;
      }
      if (uVar18 < uVar33) goto LAB_1027a4e84;
      dVar19 = (double)(uVar18 - uVar33);
      if (uVar34 - uVar33 <= uVar18 - uVar33) {
        dVar19 = (double)(uVar34 - uVar33);
      }
      if (0x7ffffffffffffff7 < (ulong)dVar19) goto LAB_1027a77a4;
      if ((ulong)dVar19 < 0x17) {
        local_100 = (double)CONCAT17(SUB81(dVar19,0),(undefined7)local_100);
        pppppppuVar13 = &local_110;
        if (dVar19 != 0.0) goto LAB_1027a4d24;
                    /* WARNING: Ignoring partial resolution of indirect */
        local_110._0_1_ = 0;
        if (local_118 <= local_120) goto LAB_1027a4d44;
LAB_1027a4cdc:
        *(double *)(local_120 + 0x10) = local_100;
        *(double *)(local_120 + 8) = dStack_108;
        *(undefined8 ********)local_120 = local_110;
        local_120 = local_120 + 0x18;
      }
      else {
        uVar35 = ((ulong)dVar19 & 0xfffffffffffffff8) + 8;
        if (((ulong)dVar19 | 7) != 0x17) {
          uVar35 = (ulong)dVar19 | 7;
        }
        pppppppuVar13 = (undefined8 *******)__Znwm(uVar35 + 1);
        local_100 = (double)(uVar35 + 1 | 0x8000000000000000);
        local_110 = pppppppuVar13;
        dStack_108 = dVar19;
LAB_1027a4d24:
        _memmove(pppppppuVar13,(void *)((long)plVar24 + uVar33),(size_t)dVar19);
        *(undefined1 *)((long)pppppppuVar13 + (long)dVar19) = 0;
        if (local_120 < local_118) goto LAB_1027a4cdc;
LAB_1027a4d44:
        local_120 = (char *)FUN_1001f94c4(&local_128,&local_110);
      }
      uVar33 = uVar34 + 1;
      uVar21 = (ulong)*(byte *)((long)param_1 + 0x17);
      plVar24 = (long *)*param_1;
      uVar18 = param_1[1];
      uVar35 = uVar18;
      plVar6 = plVar24;
      if (-1 < (char)*(byte *)((long)param_1 + 0x17)) {
        uVar35 = uVar21;
        plVar6 = param_1;
      }
      uVar34 = uStack_138;
      pcVar14 = (char *)CONCAT62(uStack_13e,local_140);
      if (-1 < (char)local_129) {
        uVar34 = (ulong)local_129;
        pcVar14 = (char *)&local_140;
      }
      if ((uVar33 < uVar35) && (uVar34 != 0)) {
        pcVar17 = (char *)((long)plVar6 + uVar33);
        do {
          uVar22 = uVar34;
          pcVar23 = pcVar14;
          do {
            if (*pcVar17 == *pcVar23) {
              if ((pcVar17 == (char *)((long)plVar6 + uVar35)) ||
                 (uVar34 = (long)pcVar17 - (long)plVar6, uVar34 == 0xffffffffffffffff))
              goto LAB_1027a4df4;
              goto LAB_1027a4c80;
            }
            pcVar23 = pcVar23 + 1;
            uVar22 = uVar22 - 1;
          } while (uVar22 != 0);
          pcVar17 = pcVar17 + 1;
        } while (pcVar17 != (char *)((long)plVar6 + uVar35));
      }
    }
    goto LAB_1027a4df4;
  }
  uVar28 = 0xffffffff;
  goto LAB_1027a7734;
code_r0x0001027a4c40:
  uVar34 = uVar34 + 1;
  if (uVar35 == uVar34) goto code_r0x0001027a4c4c;
  goto LAB_1027a4c34;
code_r0x0001027a4c4c:
  uVar33 = 0;
LAB_1027a4df4:
  if ((uint)uVar21 >> 7 == 0) {
    if ((long)uVar21 <= (long)uVar33) goto LAB_1027a4ef8;
joined_r0x0001027a4e80:
    if (uVar21 < uVar33) {
LAB_1027a4e84:
                    /* WARNING: Subroutine does not return */
      FUN_10002bfd4(&local_110);
    }
    dVar19 = (double)(uVar21 - uVar33);
    if (~uVar33 <= (ulong)dVar19) {
      dVar19 = (double)~uVar33;
    }
    if (0x7ffffffffffffff7 < (ulong)dVar19) {
LAB_1027a77a4:
                    /* WARNING: Subroutine does not return */
      FUN_100016eb8(&local_110);
    }
    if ((ulong)dVar19 < 0x17) {
      local_100 = (double)CONCAT17(SUB81(dVar19,0),(undefined7)local_100);
      pppppppuVar13 = &local_110;
      if (dVar19 != 0.0) goto LAB_1027a4ec0;
                    /* WARNING: Ignoring partial resolution of indirect */
      local_110._0_1_ = 0;
      pcVar14 = local_120;
      if (local_118 <= local_120) goto LAB_1027a4ee0;
    }
    else {
      uVar35 = ((ulong)dVar19 & 0xfffffffffffffff8) + 8;
      if (((ulong)dVar19 | 7) != 0x17) {
        uVar35 = (ulong)dVar19 | 7;
      }
      pppppppuVar13 = (undefined8 *******)__Znwm(uVar35 + 1);
      local_100 = (double)(uVar35 + 1 | 0x8000000000000000);
      local_110 = pppppppuVar13;
      dStack_108 = dVar19;
LAB_1027a4ec0:
      _memmove(pppppppuVar13,(void *)((long)param_1 + uVar33),(size_t)dVar19);
      *(undefined1 *)((long)pppppppuVar13 + (long)dVar19) = 0;
      pcVar14 = local_120;
      if (local_118 <= local_120) {
LAB_1027a4ee0:
        local_120 = (char *)FUN_1001f94c4(&local_128,&local_110);
        goto LAB_1027a4ef8;
      }
    }
    *(double *)(pcVar14 + 0x10) = local_100;
    local_120 = pcVar14 + 0x18;
    *(double *)(pcVar14 + 8) = dStack_108;
    *(undefined8 ********)pcVar14 = local_110;
  }
  else {
    uVar21 = uVar18;
    param_1 = plVar24;
    if ((long)uVar33 < (long)uVar18) goto joined_r0x0001027a4e80;
LAB_1027a4ef8:
    pcVar14 = local_120 + -0x18;
  }
  pcVar17 = local_120;
  uVar7 = _atoi(pcVar14);
  pcVar14 = local_128;
  if ((uVar7 & 0xffff0000) == 0) {
    if (local_128[0x17] < '\0') {
      uVar7 = _atoi(*(char **)local_128);
    }
    else {
      uVar7 = _atoi(local_128);
    }
    if (uVar7 == 0) {
LAB_1027a5130:
      uVar28 = 1;
    }
    else {
      uVar35 = (ulong)uVar7;
      lVar29 = (long)(int)uVar7;
      lVar31 = param_2[1];
      if (lVar31 != lVar29) {
        pvVar30 = (void *)*param_2;
        if ((lVar31 != 0) && (pvVar30 != (void *)0x0)) {
          puVar20 = (undefined8 *)((long)pvVar30 + lVar31 * 0xc0);
          do {
            puVar20 = puVar20 + -0x18;
            lVar31 = lVar31 + -1;
            _free((void *)*puVar20);
          } while (lVar31 != 0);
        }
        _free(pvVar30);
        if ((int)uVar7 < 1) {
          lVar31 = 0;
        }
        else {
          lVar31 = FUN_1027a3950(lVar29);
        }
        *param_2 = lVar31;
        pcVar17 = local_120;
        pcVar14 = local_128;
      }
      param_2[1] = lVar29;
      lVar29 = ((long)pcVar17 - (long)pcVar14 >> 3) * -0x5555555555555555;
      if (uVar7 < 3) {
        uVar9 = uVar7 * 6;
        if (lVar29 - (ulong)(uVar9 + 4) != 0) {
          uVar28 = 3;
          goto LAB_1027a76dc;
        }
        uVar18 = 0;
        uVar32 = 6;
        lVar29 = 0x60;
        auVar63 = NEON_fmov(0x3ff0000000000000,8);
        do {
          pcVar14 = local_128;
          pcVar17 = local_128 + (ulong)(uVar9 | 1) * 0x18;
          if (pcVar17[0x17] < '\0') {
            pcVar17 = *(char **)pcVar17;
          }
          lVar31 = *param_2;
          iVar10 = _atoi(pcVar17);
          iVar8 = 0;
          if (uVar7 != 0) {
            iVar8 = iVar10 / (int)uVar7;
          }
          pcVar17 = pcVar14 + (ulong)(uVar9 + 2) * 0x18;
          local_dc = iVar8;
          if (pcVar17[0x17] < '\0') {
            local_d8 = _atoi(*(char **)pcVar17);
            pcVar17 = pcVar14 + (ulong)(uVar32 - 5) * 0x18;
            if (-1 < pcVar17[0x17]) goto LAB_1027a545c;
LAB_1027a5600:
            local_c0 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + (ulong)(uVar32 - 4) * 0x18;
            cVar5 = pcVar17[0x17];
          }
          else {
            local_d8 = _atoi(pcVar17);
            pcVar17 = pcVar14 + (ulong)(uVar32 - 5) * 0x18;
            if (pcVar17[0x17] < '\0') goto LAB_1027a5600;
LAB_1027a545c:
            local_c0 = _atof(pcVar17);
            pcVar17 = pcVar14 + (ulong)(uVar32 - 4) * 0x18;
            cVar5 = pcVar17[0x17];
          }
          if (cVar5 < '\0') {
            pcVar17 = *(char **)pcVar17;
          }
          local_d0 = _atof(pcVar17);
          local_d0 = local_d0 - (double)(iVar8 * (int)uVar18);
          pcVar14 = pcVar14 + (ulong)(uVar32 - 3) * 0x18;
          if (pcVar14[0x17] < '\0') {
            dStack_c8 = _atof(*(char **)pcVar14);
          }
          else {
            dStack_c8 = _atof(pcVar14);
          }
          local_e0 = _atoi(local_120 + -0x18);
          local_e0 = local_e0 & 0xff;
          FUN_1027a7828(&local_110);
          lVar27 = lVar31 + lVar29;
          local_e8 = (double)FUN_1027aa2d8(local_e0);
          dVar39 = dStack_f0;
          dVar38 = local_f8;
          dVar37 = local_100;
          dVar19 = dStack_108;
          lVar25 = lVar31 + lVar29;
          puVar20 = *(undefined8 **)(lVar27 + -0x60);
          if (*(long *)(lVar25 + -0x58) != 4) {
            _free(puVar20);
            puVar20 = _malloc(0x20);
            if (puVar20 == (undefined8 *)0x0) goto LAB_1027a7780;
            *(undefined8 **)(lVar27 + -0x60) = puVar20;
            *(undefined8 *)(lVar25 + -0x58) = 4;
          }
          pcVar14 = local_128;
          puVar20[1] = (dVar37 * 57.29577951308232) / dVar19;
          *puVar20 = auVar63._0_8_;
          puVar20[3] = (dVar39 * 188090.94881441945) / dVar19;
          puVar20[2] = (dVar38 * 3282.806350011744) / dVar19;
          dVar36 = local_e8 * 0.5;
          dVar19 = (dVar19 * local_c0 * 180.0) /
                   (dVar36 * (dVar19 + dVar36 * (dVar37 + dVar36 * (dVar38 + dVar36 * dVar39))) *
                   3.141592653589793);
          *(double *)(lVar25 + -0x48) = dVar19;
          *(double *)(lVar25 + -0x40) = dVar19;
          *(double *)(lVar25 + -0x28) = dStack_c8;
          *(double *)(lVar25 + -0x30) = local_d0;
          *(int *)(lVar25 + -0x50) = local_dc;
          *(int *)(lVar25 + -0x4c) = local_d8;
          *(uint *)(lVar25 + -0x18) = local_e0;
          *(undefined4 *)(lVar25 + -0x14) = 0;
          pcVar17 = local_128 + (ulong)(uVar32 - 2) * 0x18;
          if (pcVar17[0x17] < '\0') {
            dVar19 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + (ulong)(uVar32 - 1) * 0x18;
            if (-1 < pcVar17[0x17]) goto LAB_1027a55cc;
LAB_1027a5664:
            dVar37 = _atof(*(char **)pcVar17);
            pcVar14 = pcVar14 + (ulong)uVar32 * 0x18;
            if (pcVar14[0x17] < '\0') goto LAB_1027a51e4;
LAB_1027a567c:
            dVar38 = _atof(pcVar14);
          }
          else {
            dVar19 = _atof(pcVar17);
            pcVar17 = pcVar14 + (ulong)(uVar32 - 1) * 0x18;
            if (pcVar17[0x17] < '\0') goto LAB_1027a5664;
LAB_1027a55cc:
            dVar37 = _atof(pcVar17);
            pcVar14 = pcVar14 + (ulong)uVar32 * 0x18;
            if (-1 < pcVar14[0x17]) goto LAB_1027a567c;
LAB_1027a51e4:
            dVar38 = _atof(*(char **)pcVar14);
          }
          iVar8 = _atoi(local_120 + -0x18);
          pdVar15 = (double *)(lVar31 + lVar29);
          *(int *)(pdVar15 + 10) = iVar8 >> 8;
          dVar41 = (dVar37 * 3.141592653589793) / 180.0 + 1.5707963267948966;
          auVar42._0_8_ = dVar41 * 0.5;
          auVar42._8_8_ = 0;
          dVar37 = (double)___sincos_stret(auVar42);
          dVar39 = dVar37 * 0.0;
          dVar40 = dVar37 * 1.0;
          dVar61 = dVar37 * 0.0;
          auVar43._0_8_ = ((dVar19 * 3.141592653589793) / 180.0) * 0.5;
          auVar43._8_8_ = 0;
          dVar19 = dVar41;
          dVar36 = (double)___sincos_stret(auVar43);
          dVar59 = dVar36 * 0.0;
          dVar60 = dVar36 * 0.0;
          dVar65 = ((-dVar39 * dVar59 + dVar19 * dVar41) - dVar60 * dVar40) - dVar36 * dVar61;
          dVar64 = dVar19 * dVar39 + dVar41 * dVar59 + dVar36 * dVar40;
          dVar67 = dVar64 - dVar61 * dVar60;
          dVar62 = (dVar19 * dVar40 + dVar41 * dVar60 + dVar61 * dVar59) - dVar36 * dVar39;
          dVar19 = dVar19 * dVar61;
          dVar61 = (dVar19 + dVar36 * dVar41 + dVar60 * dVar39) - dVar59 * dVar40;
          auVar44._0_8_ = ((dVar38 * 3.141592653589793) / 180.0) * 0.5;
          auVar44._8_8_ = 0;
          dVar38 = (double)___sincos_stret(auVar44);
          dVar39 = dVar38 * 1.0;
          dVar36 = dVar38 * 0.0;
          dVar38 = dVar38 * 0.0;
          dVar40 = (((-(-(dVar37 * 0.0) * dVar60) - dVar64) * dVar39 + dVar19 * dVar65) -
                   dVar62 * dVar36) - dVar38 * dVar61;
          dVar41 = (dVar67 * dVar19 + dVar65 * dVar39 + dVar38 * dVar62) - dVar61 * dVar36;
          dVar59 = (dVar62 * dVar19 + dVar65 * dVar36 + dVar61 * dVar39) - dVar38 * dVar67;
          dVar19 = (dVar61 * dVar19 + dVar38 * dVar65 + dVar67 * dVar36) - dVar62 * dVar39;
          dVar37 = ((dVar40 - dVar41 * 0.0) - dVar59 * 0.0) - dVar19 * 0.0;
          dVar38 = (dVar41 + dVar40 * 0.0 + dVar59 * 0.0) - dVar19 * 0.0;
          dVar60 = (dVar59 + dVar40 * 0.0 + dVar19 * 0.0) - dVar41 * 0.0;
          dVar19 = (dVar19 + dVar40 * 0.0 + dVar41 * 0.0) - dVar59 * 0.0;
          dVar36 = dVar60 + dVar60;
          dVar40 = dVar19 + dVar19;
          dVar41 = dVar37 * (dVar38 + dVar38);
          dVar39 = dVar38 * (dVar38 + dVar38);
          *(double *)(lVar27 + -8) = dVar38 * dVar36 + dVar37 * dVar40;
          *(double *)(lVar27 + -0x10) = 1.0 - (dVar60 * dVar36 + dVar19 * dVar40);
          pdVar15[1] = dVar38 * dVar36 - dVar37 * dVar40;
          *pdVar15 = dVar38 * dVar40 - dVar37 * dVar36;
          pdVar15[3] = dVar60 * dVar40 + dVar41;
          pdVar15[2] = 1.0 - (dVar39 + dVar19 * dVar40);
          pdVar15[5] = dVar60 * dVar40 - dVar41;
          pdVar15[4] = dVar38 * dVar40 + dVar37 * dVar36;
          pdVar15[6] = 1.0 - (dVar39 + dVar60 * dVar36);
          uVar18 = uVar18 + 1;
          uVar32 = uVar32 + 6;
          lVar29 = lVar29 + 0xc0;
        } while (uVar35 != uVar18);
      }
      else if (lVar29 - (ulong)(uVar7 * 0xc | 2) == 0) {
        if (0 < (int)uVar7) {
          lVar29 = 0;
          lVar31 = 0xb0;
          auVar63 = NEON_fmov(0x3ff0000000000000,8);
          do {
            pcVar17 = local_128;
            pcVar14 = local_128 + lVar29 + 0x18;
            if (local_128[lVar29 + 0x2f] < '\0') {
              pcVar14 = *(char **)pcVar14;
            }
            lVar27 = *param_2;
            local_c0 = _atof(pcVar14);
            if (pcVar17[lVar29 + 0x47] < '\0') {
              local_d0 = _atof(*(char **)(pcVar17 + lVar29 + 0x30));
              pcVar14 = pcVar17 + lVar29 + 0x48;
              if (-1 < pcVar17[lVar29 + 0x5f]) goto LAB_1027a5784;
LAB_1027a5b50:
              dStack_c8 = _atof(*(char **)pcVar14);
              cVar5 = pcVar17[lVar29 + 0xef];
            }
            else {
              local_d0 = _atof(pcVar17 + lVar29 + 0x30);
              pcVar14 = pcVar17 + lVar29 + 0x48;
              if (pcVar17[lVar29 + 0x5f] < '\0') goto LAB_1027a5b50;
LAB_1027a5784:
              dStack_c8 = _atof(pcVar14);
              cVar5 = pcVar17[lVar29 + 0xef];
            }
            pcVar14 = pcVar17 + lVar29 + 0xd8;
            if (cVar5 < '\0') {
              pcVar14 = *(char **)pcVar14;
            }
            uVar7 = _atoi(pcVar14);
            local_e0 = uVar7;
            if (pcVar17[lVar29 + 0xbf] < '\0') {
              local_dc = _atoi(*(char **)(pcVar17 + lVar29 + 0xa8));
              cVar5 = pcVar17[lVar29 + 0xd7];
            }
            else {
              local_dc = _atoi(pcVar17 + lVar29 + 0xa8);
              cVar5 = pcVar17[lVar29 + 0xd7];
            }
            pcVar14 = pcVar17 + lVar29 + 0xc0;
            if (cVar5 < '\0') {
              pcVar14 = *(char **)(pcVar17 + lVar29 + 0xc0);
            }
            local_d8 = _atoi(pcVar14);
            FUN_1027a7828(&local_110,uVar7);
            lVar25 = lVar27 + lVar31;
            local_e8 = (double)FUN_1027aa2d8(local_e0);
            dVar39 = dStack_f0;
            dVar38 = local_f8;
            dVar37 = local_100;
            dVar19 = dStack_108;
            lVar1 = lVar27 + lVar31;
            puVar20 = *(undefined8 **)(lVar25 + -0xb0);
            if (*(long *)(lVar1 + -0xa8) != 4) {
              _free(puVar20);
              puVar20 = _malloc(0x20);
              if (puVar20 == (undefined8 *)0x0) goto LAB_1027a7780;
              *(undefined8 **)(lVar25 + -0xb0) = puVar20;
              *(undefined8 *)(lVar1 + -0xa8) = 4;
            }
            pcVar14 = local_128;
            puVar20[1] = (dVar37 * 57.29577951308232) / dVar19;
            *puVar20 = auVar63._0_8_;
            puVar20[3] = (dVar39 * 188090.94881441945) / dVar19;
            puVar20[2] = (dVar38 * 3282.806350011744) / dVar19;
            dVar36 = local_e8 * 0.5;
            dVar19 = (dVar19 * local_c0 * 180.0) /
                     (dVar36 * (dVar19 + dVar36 * (dVar37 + dVar36 * (dVar38 + dVar36 * dVar39))) *
                     3.141592653589793);
            *(double *)(lVar1 + -0x98) = dVar19;
            *(double *)(lVar1 + -0x90) = dVar19;
            *(double *)(lVar1 + -0x78) = dStack_c8;
            *(double *)(lVar1 + -0x80) = local_d0;
            *(int *)(lVar1 + -0xa0) = local_dc;
            *(int *)(lVar1 + -0x9c) = local_d8;
            *(uint *)(lVar1 + -0x68) = local_e0;
            *(undefined4 *)(lVar1 + -100) = 0;
            if (local_128[lVar29 + 0x77] < '\0') {
              dVar19 = _atof(*(char **)(local_128 + lVar29 + 0x60));
              if (-1 < pcVar14[lVar29 + 0x8f]) goto LAB_1027a58e0;
LAB_1027a5ba8:
              dVar37 = _atof(*(char **)(pcVar14 + lVar29 + 0x78));
              pcVar17 = pcVar14 + lVar29 + 0x90;
              if (-1 < pcVar14[lVar29 + 0xa7]) goto LAB_1027a58f8;
LAB_1027a5bc4:
              dVar38 = _atof(*(char **)pcVar17);
              cVar5 = pcVar14[lVar29 + 0x107];
            }
            else {
              dVar19 = _atof(local_128 + lVar29 + 0x60);
              if (pcVar14[lVar29 + 0x8f] < '\0') goto LAB_1027a5ba8;
LAB_1027a58e0:
              dVar37 = _atof(pcVar14 + lVar29 + 0x78);
              pcVar17 = pcVar14 + lVar29 + 0x90;
              if (pcVar14[lVar29 + 0xa7] < '\0') goto LAB_1027a5bc4;
LAB_1027a58f8:
              dVar38 = _atof(pcVar17);
              cVar5 = pcVar14[lVar29 + 0x107];
            }
            pcVar17 = pcVar14 + lVar29 + 0xf0;
            if (cVar5 < '\0') {
              pcVar17 = *(char **)pcVar17;
            }
            dVar39 = _atof(pcVar17);
            lVar1 = lVar27 + lVar31;
            *(double *)(lVar1 + -0x18) = dVar39;
            if (pcVar14[lVar29 + 0x11f] < '\0') {
              dVar39 = _atof(*(char **)(pcVar14 + lVar29 + 0x108));
              *(double *)(lVar1 + -0x10) = dVar39;
              cVar5 = pcVar14[lVar29 + 0x137];
            }
            else {
              dVar39 = _atof(pcVar14 + lVar29 + 0x108);
              *(double *)(lVar1 + -0x10) = dVar39;
              cVar5 = pcVar14[lVar29 + 0x137];
            }
            pcVar17 = pcVar14 + lVar29 + 0x120;
            if (cVar5 < '\0') {
              pcVar17 = *(char **)(pcVar14 + lVar29 + 0x120);
            }
            dVar39 = _atof(pcVar17);
            lVar1 = lVar27 + lVar31;
            *(double *)(lVar1 + -8) = dVar39;
            dVar41 = (dVar37 * 3.141592653589793) / 180.0 + 1.5707963267948966;
            auVar45._0_8_ = dVar41 * 0.5;
            auVar45._8_8_ = 0;
            dVar37 = (double)___sincos_stret(auVar45);
            dVar39 = dVar37 * 0.0;
            dVar40 = dVar37 * 1.0;
            dVar61 = dVar37 * 0.0;
            auVar46._0_8_ = ((dVar19 * 3.141592653589793) / 180.0) * 0.5;
            auVar46._8_8_ = 0;
            dVar19 = dVar41;
            dVar36 = (double)___sincos_stret(auVar46);
            dVar59 = dVar36 * 0.0;
            dVar60 = dVar36 * 0.0;
            dVar65 = ((-dVar39 * dVar59 + dVar19 * dVar41) - dVar60 * dVar40) - dVar36 * dVar61;
            dVar64 = dVar19 * dVar39 + dVar41 * dVar59 + dVar36 * dVar40;
            dVar67 = dVar64 - dVar61 * dVar60;
            dVar62 = (dVar19 * dVar40 + dVar41 * dVar60 + dVar61 * dVar59) - dVar36 * dVar39;
            dVar19 = dVar19 * dVar61;
            dVar61 = (dVar19 + dVar36 * dVar41 + dVar60 * dVar39) - dVar59 * dVar40;
            auVar47._0_8_ = ((dVar38 * 3.141592653589793) / 180.0) * 0.5;
            auVar47._8_8_ = 0;
            dVar38 = (double)___sincos_stret(auVar47);
            dVar39 = dVar38 * 1.0;
            dVar36 = dVar38 * 0.0;
            dVar38 = dVar38 * 0.0;
            dVar40 = (((-(-(dVar37 * 0.0) * dVar60) - dVar64) * dVar39 + dVar19 * dVar65) -
                     dVar62 * dVar36) - dVar38 * dVar61;
            dVar41 = (dVar67 * dVar19 + dVar65 * dVar39 + dVar38 * dVar62) - dVar61 * dVar36;
            dVar59 = (dVar62 * dVar19 + dVar65 * dVar36 + dVar61 * dVar39) - dVar38 * dVar67;
            dVar19 = (dVar61 * dVar19 + dVar38 * dVar65 + dVar67 * dVar36) - dVar62 * dVar39;
            dVar37 = ((dVar40 - dVar41 * 0.0) - dVar59 * 0.0) - dVar19 * 0.0;
            dVar38 = (dVar41 + dVar40 * 0.0 + dVar59 * 0.0) - dVar19 * 0.0;
            dVar60 = (dVar59 + dVar40 * 0.0 + dVar19 * 0.0) - dVar41 * 0.0;
            dVar19 = (dVar19 + dVar40 * 0.0 + dVar41 * 0.0) - dVar59 * 0.0;
            dVar36 = dVar60 + dVar60;
            dVar40 = dVar19 + dVar19;
            dVar41 = dVar37 * (dVar38 + dVar38);
            dVar39 = dVar38 * (dVar38 + dVar38);
            *(double *)(lVar25 + -0x58) = dVar38 * dVar36 + dVar37 * dVar40;
            *(double *)(lVar25 + -0x60) = 1.0 - (dVar60 * dVar36 + dVar19 * dVar40);
            *(double *)(lVar1 + -0x48) = dVar38 * dVar36 - dVar37 * dVar40;
            *(double *)(lVar1 + -0x50) = dVar38 * dVar40 - dVar37 * dVar36;
            *(double *)(lVar1 + -0x38) = dVar60 * dVar40 + dVar41;
            *(double *)(lVar1 + -0x40) = 1.0 - (dVar39 + dVar19 * dVar40);
            *(double *)(lVar1 + -0x28) = dVar60 * dVar40 - dVar41;
            *(double *)(lVar1 + -0x30) = dVar38 * dVar40 + dVar37 * dVar36;
            *(double *)(lVar1 + -0x20) = 1.0 - (dVar39 + dVar60 * dVar36);
            iVar8 = _atoi(local_120 + -0x18);
            *(int *)(lVar27 + lVar31) = iVar8;
            lVar29 = lVar29 + 0x120;
            lVar31 = lVar31 + 0xc0;
          } while (uVar35 * 0x120 - lVar29 != 0);
        }
      }
      else {
        if (lVar29 - (ulong)(uVar7 * 9 + 2) != 0) goto LAB_1027a76a4;
        if (0 < (int)uVar7) {
          lVar29 = 0;
          lVar31 = 0xb0;
          auVar63 = NEON_fmov(0x3ff0000000000000,8);
          do {
            pcVar17 = local_128;
            pcVar14 = local_128 + lVar29 + 0xa8;
            if (local_128[lVar29 + 0xbf] < '\0') {
              pcVar14 = *(char **)pcVar14;
            }
            lVar27 = *param_2;
            dVar19 = _atof(pcVar14);
            local_dc = (int)dVar19;
            if (pcVar17[lVar29 + 0xd7] < '\0') {
              local_d8 = _atoi(*(char **)(pcVar17 + lVar29 + 0xc0));
              pcVar14 = pcVar17 + lVar29 + 0x18;
              if (-1 < pcVar17[lVar29 + 0x2f]) goto LAB_1027a5cf8;
LAB_1027a6078:
              local_c0 = _atof(*(char **)pcVar14);
              pcVar14 = pcVar17 + lVar29 + 0x30;
              if (-1 < pcVar17[lVar29 + 0x47]) goto LAB_1027a5d0c;
LAB_1027a6090:
              local_d0 = _atof(*(char **)pcVar14);
              pcVar14 = pcVar17 + lVar29 + 0x48;
              if (-1 < pcVar17[lVar29 + 0x5f]) goto LAB_1027a5d24;
LAB_1027a60ac:
              dStack_c8 = _atof(*(char **)pcVar14);
              cVar5 = pcVar17[lVar29 + 0xef];
            }
            else {
              local_d8 = _atoi(pcVar17 + lVar29 + 0xc0);
              pcVar14 = pcVar17 + lVar29 + 0x18;
              if (pcVar17[lVar29 + 0x2f] < '\0') goto LAB_1027a6078;
LAB_1027a5cf8:
              local_c0 = _atof(pcVar14);
              pcVar14 = pcVar17 + lVar29 + 0x30;
              if (pcVar17[lVar29 + 0x47] < '\0') goto LAB_1027a6090;
LAB_1027a5d0c:
              local_d0 = _atof(pcVar14);
              pcVar14 = pcVar17 + lVar29 + 0x48;
              if (pcVar17[lVar29 + 0x5f] < '\0') goto LAB_1027a60ac;
LAB_1027a5d24:
              dStack_c8 = _atof(pcVar14);
              cVar5 = pcVar17[lVar29 + 0xef];
            }
            pcVar17 = pcVar17 + lVar29 + 0xd8;
            if (cVar5 < '\0') {
              pcVar17 = *(char **)pcVar17;
            }
            local_e0 = _atoi(pcVar17);
            FUN_1027a7828(&local_110,local_e0);
            lVar25 = lVar27 + lVar31;
            local_e8 = (double)FUN_1027aa2d8(local_e0);
            dVar39 = dStack_f0;
            dVar38 = local_f8;
            dVar37 = local_100;
            dVar19 = dStack_108;
            lVar1 = lVar27 + lVar31;
            puVar20 = *(undefined8 **)(lVar25 + -0xb0);
            if (*(long *)(lVar1 + -0xa8) != 4) {
              _free(puVar20);
              puVar20 = _malloc(0x20);
              if (puVar20 == (undefined8 *)0x0) goto LAB_1027a7780;
              *(undefined8 **)(lVar25 + -0xb0) = puVar20;
              *(undefined8 *)(lVar1 + -0xa8) = 4;
            }
            pcVar14 = local_128;
            puVar20[1] = (dVar37 * 57.29577951308232) / dVar19;
            *puVar20 = auVar63._0_8_;
            puVar20[3] = (dVar39 * 188090.94881441945) / dVar19;
            puVar20[2] = (dVar38 * 3282.806350011744) / dVar19;
            dVar36 = local_e8 * 0.5;
            dVar19 = (dVar19 * local_c0 * 180.0) /
                     (dVar36 * (dVar19 + dVar36 * (dVar37 + dVar36 * (dVar38 + dVar36 * dVar39))) *
                     3.141592653589793);
            *(double *)(lVar1 + -0x98) = dVar19;
            *(double *)(lVar1 + -0x90) = dVar19;
            *(double *)(lVar1 + -0x78) = dStack_c8;
            *(double *)(lVar1 + -0x80) = local_d0;
            *(int *)(lVar1 + -0xa0) = local_dc;
            *(int *)(lVar1 + -0x9c) = local_d8;
            *(uint *)(lVar1 + -0x68) = local_e0;
            *(undefined4 *)(lVar1 + -100) = 0;
            if (local_128[lVar29 + 0x77] < '\0') {
              dVar19 = _atof(*(char **)(local_128 + lVar29 + 0x60));
              if (-1 < pcVar14[lVar29 + 0x8f]) goto LAB_1027a5e50;
LAB_1027a60e4:
              dVar37 = _atof(*(char **)(pcVar14 + lVar29 + 0x78));
              cVar5 = pcVar14[lVar29 + 0xa7];
            }
            else {
              dVar19 = _atof(local_128 + lVar29 + 0x60);
              if (pcVar14[lVar29 + 0x8f] < '\0') goto LAB_1027a60e4;
LAB_1027a5e50:
              dVar37 = _atof(pcVar14 + lVar29 + 0x78);
              cVar5 = pcVar14[lVar29 + 0xa7];
            }
            pcVar14 = pcVar14 + lVar29 + 0x90;
            if (cVar5 < '\0') {
              pcVar14 = *(char **)pcVar14;
            }
            dVar38 = _atof(pcVar14);
            auVar48._0_8_ = ((dVar37 * 3.141592653589793) / 180.0 + 1.5707963267948966) * 0.5;
            auVar48._8_8_ = 0;
            dVar37 = 3.141592653589793;
            dVar39 = (double)___sincos_stret(auVar48);
            dVar36 = dVar39 * 0.0;
            dVar41 = dVar39 * 1.0;
            dVar65 = dVar39 * 0.0;
            auVar49._0_8_ = ((dVar19 * 3.141592653589793) / 180.0) * 0.5;
            auVar49._8_8_ = 0;
            dVar19 = dVar37;
            dVar40 = (double)___sincos_stret(auVar49);
            dVar59 = dVar40 * 0.0;
            dVar60 = dVar40 * 0.0;
            dVar64 = ((-dVar36 * dVar59 + dVar19 * dVar37) - dVar60 * dVar41) - dVar40 * dVar65;
            dVar61 = dVar19 * dVar36 + dVar37 * dVar59 + dVar40 * dVar41;
            dVar62 = dVar61 - dVar65 * dVar60;
            dVar67 = (dVar19 * dVar41 + dVar37 * dVar60 + dVar65 * dVar59) - dVar40 * dVar36;
            dVar19 = dVar19 * dVar65;
            dVar59 = (dVar19 + dVar40 * dVar37 + dVar60 * dVar36) - dVar59 * dVar41;
            auVar50._0_8_ = ((dVar38 * 3.141592653589793) / 180.0) * 0.5;
            auVar50._8_8_ = 0;
            dVar37 = (double)___sincos_stret(auVar50);
            dVar38 = dVar37 * 1.0;
            dVar36 = dVar37 * 0.0;
            dVar37 = dVar37 * 0.0;
            dVar39 = (((-(-(dVar39 * 0.0) * dVar60) - dVar61) * dVar38 + dVar19 * dVar64) -
                     dVar67 * dVar36) - dVar37 * dVar59;
            dVar40 = (dVar62 * dVar19 + dVar64 * dVar38 + dVar37 * dVar67) - dVar59 * dVar36;
            dVar41 = (dVar67 * dVar19 + dVar64 * dVar36 + dVar59 * dVar38) - dVar37 * dVar62;
            dVar19 = (dVar59 * dVar19 + dVar37 * dVar64 + dVar62 * dVar36) - dVar67 * dVar38;
            dVar37 = ((dVar39 - dVar40 * 0.0) - dVar41 * 0.0) - dVar19 * 0.0;
            dVar38 = (dVar40 + dVar39 * 0.0 + dVar41 * 0.0) - dVar19 * 0.0;
            dVar59 = (dVar41 + dVar39 * 0.0 + dVar19 * 0.0) - dVar40 * 0.0;
            dVar19 = (dVar19 + dVar39 * 0.0 + dVar40 * 0.0) - dVar41 * 0.0;
            dVar36 = dVar59 + dVar59;
            dVar40 = dVar19 + dVar19;
            dVar41 = dVar37 * (dVar38 + dVar38);
            dVar39 = dVar38 * (dVar38 + dVar38);
            *(double *)(lVar25 + -0x58) = dVar38 * dVar36 + dVar37 * dVar40;
            *(double *)(lVar25 + -0x60) = 1.0 - (dVar59 * dVar36 + dVar19 * dVar40);
            lVar25 = lVar27 + lVar31;
            *(double *)(lVar25 + -0x48) = dVar38 * dVar36 - dVar37 * dVar40;
            *(double *)(lVar25 + -0x50) = dVar38 * dVar40 - dVar37 * dVar36;
            *(double *)(lVar25 + -0x38) = dVar59 * dVar40 + dVar41;
            *(double *)(lVar25 + -0x40) = 1.0 - (dVar39 + dVar19 * dVar40);
            *(double *)(lVar25 + -0x28) = dVar59 * dVar40 - dVar41;
            *(double *)(lVar25 + -0x30) = dVar38 * dVar40 + dVar37 * dVar36;
            *(double *)(lVar25 + -0x20) = 1.0 - (dVar39 + dVar59 * dVar36);
            iVar8 = _atoi(local_120 + -0x18);
            *(int *)(lVar27 + lVar31) = iVar8;
            lVar29 = lVar29 + 0xd8;
            lVar31 = lVar31 + 0xc0;
          } while (uVar35 * 0xd8 - lVar29 != 0);
        }
      }
      if ((param_2[1] == 2) && (lVar29 = *param_2, *(int *)(lVar29 + 0xb0) == 5))
      goto LAB_1027a76c8;
LAB_1027a76d8:
      uVar28 = 0;
    }
  }
  else {
    iVar8 = (int)uVar7 >> 0x10;
    if (iVar8 == 6) {
      if (local_128[0x17] < '\0') {
        pcVar14 = *(char **)local_128;
      }
      uVar7 = _atoi(pcVar14);
      if (uVar7 == 0) goto LAB_1027a5130;
      lVar29 = (long)(int)uVar7;
      lVar31 = param_2[1];
      if (lVar31 != lVar29) {
        pvVar30 = (void *)*param_2;
        if ((lVar31 != 0) && (pvVar30 != (void *)0x0)) {
          puVar20 = (undefined8 *)((long)pvVar30 + lVar31 * 0xc0);
          do {
            puVar20 = puVar20 + -0x18;
            lVar31 = lVar31 + -1;
            _free((void *)*puVar20);
          } while (lVar31 != 0);
        }
        _free(pvVar30);
        if ((int)uVar7 < 1) {
          lVar31 = 0;
        }
        else {
          lVar31 = FUN_1027a3950(lVar29);
        }
        *param_2 = lVar31;
        pcVar17 = local_120;
      }
      param_2[1] = lVar29;
      uVar9 = _atoi(pcVar17 + -0x18);
      if (uVar7 * 0x1b + 2 != (int)((ulong)((long)pcVar17 - (long)local_128) >> 3) * -0x55555555)
      goto LAB_1027a76a4;
      if (0 < (int)uVar7) {
        lVar29 = 0;
        iVar8 = 0;
        local_230 = uVar9 >> 8 & 0xff;
        lVar31 = 0xb0;
        do {
          pcVar17 = local_128;
          pcVar14 = local_128 + lVar29 + 0x18;
          if (local_128[lVar29 + 0x2f] < '\0') {
            pcVar14 = *(char **)pcVar14;
          }
          lVar25 = *param_2;
          lVar27 = lVar25 + lVar31;
          dVar19 = _atof(pcVar14);
          *(double *)(lVar27 + -0x70) = dVar19;
          pcVar14 = pcVar17 + lVar29 + 0x30;
          if (pcVar17[lVar29 + 0x47] < '\0') {
            pcVar14 = *(char **)pcVar14;
          }
          dVar19 = _atof(pcVar14);
          lVar1 = lVar25 + lVar31;
          *(double *)(lVar1 + -0x98) = dVar19;
          if (pcVar17[lVar29 + 0x5f] < '\0') {
            dVar19 = _atof(*(char **)(pcVar17 + lVar29 + 0x48));
            *(double *)(lVar1 + -0x90) = dVar19;
            cVar5 = pcVar17[lVar29 + 0x77];
          }
          else {
            dVar19 = _atof(pcVar17 + lVar29 + 0x48);
            *(double *)(lVar1 + -0x90) = dVar19;
            cVar5 = pcVar17[lVar29 + 0x77];
          }
          pcVar14 = pcVar17 + lVar29 + 0x60;
          if (cVar5 < '\0') {
            pcVar14 = *(char **)(pcVar17 + lVar29 + 0x60);
          }
          dVar19 = _atof(pcVar14);
          lVar1 = lVar25 + lVar31;
          *(double *)(lVar1 + -0x80) = dVar19;
          if (pcVar17[lVar29 + 0x8f] < '\0') {
            dVar19 = _atof(*(char **)(pcVar17 + lVar29 + 0x78));
            *(double *)(lVar1 + -0x78) = dVar19;
            if (-1 < pcVar17[lVar29 + 0xa7]) goto LAB_1027a6b0c;
LAB_1027a6dd8:
            dVar19 = _atof(*(char **)(pcVar17 + lVar29 + 0x90));
            pcVar14 = pcVar17 + lVar29 + 0xa8;
            if (-1 < pcVar17[lVar29 + 0xbf]) goto LAB_1027a6b24;
LAB_1027a6df4:
            dVar37 = _atof(*(char **)pcVar14);
            pcVar14 = pcVar17 + lVar29 + 0xc0;
            if (-1 < pcVar17[lVar29 + 0xd7]) goto LAB_1027a6b38;
LAB_1027a6e0c:
            dVar38 = _atof(*(char **)pcVar14);
            pcVar14 = pcVar17 + lVar29 + 0xd8;
            if (-1 < pcVar17[lVar29 + 0xef]) goto LAB_1027a6b50;
LAB_1027a6e28:
            local_160 = _atof(*(char **)pcVar14);
            pcVar14 = pcVar17 + lVar29 + 0xf0;
            if (-1 < pcVar17[lVar29 + 0x107]) goto LAB_1027a6b64;
LAB_1027a6e40:
            dVar39 = _atof(*(char **)pcVar14);
            cVar5 = pcVar17[lVar29 + 0x11f];
          }
          else {
            dVar19 = _atof(pcVar17 + lVar29 + 0x78);
            *(double *)(lVar1 + -0x78) = dVar19;
            if (pcVar17[lVar29 + 0xa7] < '\0') goto LAB_1027a6dd8;
LAB_1027a6b0c:
            dVar19 = _atof(pcVar17 + lVar29 + 0x90);
            pcVar14 = pcVar17 + lVar29 + 0xa8;
            if (pcVar17[lVar29 + 0xbf] < '\0') goto LAB_1027a6df4;
LAB_1027a6b24:
            dVar37 = _atof(pcVar14);
            pcVar14 = pcVar17 + lVar29 + 0xc0;
            if (pcVar17[lVar29 + 0xd7] < '\0') goto LAB_1027a6e0c;
LAB_1027a6b38:
            dVar38 = _atof(pcVar14);
            pcVar14 = pcVar17 + lVar29 + 0xd8;
            if (pcVar17[lVar29 + 0xef] < '\0') goto LAB_1027a6e28;
LAB_1027a6b50:
            local_160 = _atof(pcVar14);
            pcVar14 = pcVar17 + lVar29 + 0xf0;
            if (pcVar17[lVar29 + 0x107] < '\0') goto LAB_1027a6e40;
LAB_1027a6b64:
            dVar39 = _atof(pcVar14);
            cVar5 = pcVar17[lVar29 + 0x11f];
          }
          pcVar17 = pcVar17 + lVar29 + 0x108;
          if (cVar5 < '\0') {
            pcVar17 = *(char **)pcVar17;
          }
          dVar36 = _atof(pcVar17);
          pcVar14 = local_128;
          lVar2 = lVar25 + lVar31;
          *(double *)(lVar2 + -0x10) = dVar39;
          *(double *)(lVar2 + -0x18) = local_160;
          *(double *)(lVar2 + -8) = dVar36;
          if (local_128[lVar29 + 0x137] < '\0') {
            local_210 = _atof(*(char **)(local_128 + lVar29 + 0x120));
            if (-1 < pcVar14[lVar29 + 0x14f]) goto LAB_1027a6bbc;
LAB_1027a6e7c:
            local_170 = _atof(*(char **)(pcVar14 + lVar29 + 0x138));
            pcVar17 = pcVar14 + lVar29 + 0x150;
            if (-1 < pcVar14[lVar29 + 0x167]) goto LAB_1027a6bd4;
LAB_1027a6e98:
            local_160 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x168;
            if (-1 < pcVar14[lVar29 + 0x17f]) goto LAB_1027a6be8;
LAB_1027a6eb0:
            local_190 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x180;
            if (-1 < pcVar14[lVar29 + 0x197]) goto LAB_1027a6c00;
LAB_1027a6ecc:
            local_180 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x198;
            if (-1 < pcVar14[lVar29 + 0x1af]) goto LAB_1027a6c14;
LAB_1027a6ee4:
            local_1a0 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x1b0;
            if (-1 < pcVar14[lVar29 + 0x1c7]) goto LAB_1027a6c2c;
LAB_1027a6f00:
            local_1b0 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x1c8;
            if (-1 < pcVar14[lVar29 + 0x1df]) goto LAB_1027a6c40;
LAB_1027a6f18:
            local_1c0 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x1e0;
            if (-1 < pcVar14[lVar29 + 0x1f7]) goto LAB_1027a6c58;
LAB_1027a6f34:
            local_1d0 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x1f8;
            if (-1 < pcVar14[lVar29 + 0x20f]) goto LAB_1027a6c6c;
LAB_1027a6f4c:
            local_1e0 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x210;
            if (-1 < pcVar14[lVar29 + 0x227]) goto LAB_1027a6c84;
LAB_1027a6f68:
            local_1f0 = _atof(*(char **)pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x228;
            if (-1 < pcVar14[lVar29 + 0x23f]) goto LAB_1027a6c98;
LAB_1027a6f80:
            dVar39 = _atof(*(char **)pcVar17);
            cVar5 = pcVar14[lVar29 + 599];
          }
          else {
            local_210 = _atof(local_128 + lVar29 + 0x120);
            if (pcVar14[lVar29 + 0x14f] < '\0') goto LAB_1027a6e7c;
LAB_1027a6bbc:
            local_170 = _atof(pcVar14 + lVar29 + 0x138);
            pcVar17 = pcVar14 + lVar29 + 0x150;
            if (pcVar14[lVar29 + 0x167] < '\0') goto LAB_1027a6e98;
LAB_1027a6bd4:
            local_160 = _atof(pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x168;
            if (pcVar14[lVar29 + 0x17f] < '\0') goto LAB_1027a6eb0;
LAB_1027a6be8:
            local_190 = _atof(pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x180;
            if (pcVar14[lVar29 + 0x197] < '\0') goto LAB_1027a6ecc;
LAB_1027a6c00:
            local_180 = _atof(pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x198;
            if (pcVar14[lVar29 + 0x1af] < '\0') goto LAB_1027a6ee4;
LAB_1027a6c14:
            local_1a0 = _atof(pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x1b0;
            if (pcVar14[lVar29 + 0x1c7] < '\0') goto LAB_1027a6f00;
LAB_1027a6c2c:
            local_1b0 = _atof(pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x1c8;
            if (pcVar14[lVar29 + 0x1df] < '\0') goto LAB_1027a6f18;
LAB_1027a6c40:
            local_1c0 = _atof(pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x1e0;
            if (pcVar14[lVar29 + 0x1f7] < '\0') goto LAB_1027a6f34;
LAB_1027a6c58:
            local_1d0 = _atof(pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x1f8;
            if (pcVar14[lVar29 + 0x20f] < '\0') goto LAB_1027a6f4c;
LAB_1027a6c6c:
            local_1e0 = _atof(pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x210;
            if (pcVar14[lVar29 + 0x227] < '\0') goto LAB_1027a6f68;
LAB_1027a6c84:
            local_1f0 = _atof(pcVar17);
            pcVar17 = pcVar14 + lVar29 + 0x228;
            if (pcVar14[lVar29 + 0x23f] < '\0') goto LAB_1027a6f80;
LAB_1027a6c98:
            dVar39 = _atof(pcVar17);
            cVar5 = pcVar14[lVar29 + 599];
          }
          pcVar14 = pcVar14 + lVar29 + 0x240;
          if (cVar5 < '\0') {
            pcVar14 = *(char **)pcVar14;
          }
          dVar36 = _atof(pcVar14);
          pdVar15 = *(double **)(lVar27 + -0xb0);
          if (*(long *)(lVar25 + lVar31 + -0xa8) != 0xd) {
            _free(pdVar15);
            pdVar15 = _malloc(0x68);
            if (pdVar15 == (double *)0x0) goto LAB_1027a7780;
            *(double **)(lVar27 + -0xb0) = pdVar15;
            *(undefined8 *)(lVar25 + lVar31 + -0xa8) = 0xd;
          }
          pcVar17 = local_128;
          pdVar15[1] = local_170;
          *pdVar15 = local_210;
          pdVar15[3] = local_190;
          pdVar15[2] = local_160;
          pdVar15[5] = local_1a0;
          pdVar15[4] = local_180;
          pdVar15[7] = local_1c0;
          pdVar15[6] = local_1b0;
          pdVar15[9] = local_1e0;
          pdVar15[8] = local_1d0;
          pdVar15[0xb] = dVar39;
          pdVar15[10] = local_1f0;
          pdVar15[0xc] = dVar36;
          pcVar14 = local_128 + lVar29 + 600;
          if (local_128[lVar29 + 0x26f] < '\0') {
            pcVar14 = *(char **)pcVar14;
          }
          iVar10 = _atoi(pcVar14);
          lVar2 = lVar25 + lVar31;
          *(int *)(lVar2 + -0xa0) = iVar10;
          pcVar14 = pcVar17 + lVar29 + 0x270;
          if (pcVar17[lVar29 + 0x287] < '\0') {
            pcVar14 = *(char **)pcVar14;
          }
          iVar11 = _atoi(pcVar14);
          *(int *)(lVar2 + -0x9c) = iVar11;
          if (pcVar17[lVar29 + 0x29f] < '\0') {
            iVar11 = _atoi(*(char **)(pcVar17 + lVar29 + 0x288));
            *(int *)(lVar25 + lVar31 + -0x68) = iVar11;
            *(undefined4 *)(lVar25 + lVar31 + -100) = 6;
          }
          else {
            iVar11 = _atoi(pcVar17 + lVar29 + 0x288);
            *(int *)(lVar25 + lVar31 + -0x68) = iVar11;
            *(undefined4 *)(lVar25 + lVar31 + -100) = 6;
          }
          if (uVar7 == 2) {
            if (iVar10 < 0) {
              iVar10 = iVar10 + 1;
            }
            *(int *)(lVar2 + -0xa0) = iVar10 >> 1;
            *(double *)(lVar1 + -0x80) =
                 *(double *)(lVar1 + -0x80) - (double)((iVar10 >> 1) * iVar8);
          }
          puVar26 = (uint *)(lVar25 + lVar31);
          dVar41 = (dVar37 * 3.141592653589793) / 180.0 + 1.5707963267948966;
          auVar63._0_8_ = dVar41 * 0.5;
          auVar63._8_8_ = 0;
          dVar37 = (double)___sincos_stret(auVar63);
          dVar39 = dVar37 * 0.0;
          dVar40 = dVar37 * 1.0;
          dVar61 = dVar37 * 0.0;
          auVar54._0_8_ = ((dVar19 * 3.141592653589793) / 180.0) * 0.5;
          auVar54._8_8_ = 0;
          dVar19 = dVar41;
          dVar36 = (double)___sincos_stret(auVar54);
          dVar59 = dVar36 * 0.0;
          dVar60 = dVar36 * 0.0;
          dVar64 = ((-dVar39 * dVar59 + dVar19 * dVar41) - dVar60 * dVar40) - dVar36 * dVar61;
          dVar65 = dVar19 * dVar39 + dVar41 * dVar59 + dVar36 * dVar40;
          dVar67 = dVar65 - dVar61 * dVar60;
          dVar62 = (dVar19 * dVar40 + dVar41 * dVar60 + dVar61 * dVar59) - dVar36 * dVar39;
          dVar61 = (dVar19 * dVar61 + dVar36 * dVar41 + dVar60 * dVar39) - dVar59 * dVar40;
          dVar39 = 0.5;
          auVar55._0_8_ = ((dVar38 * 3.141592653589793) / 180.0) * 0.5;
          auVar55._8_8_ = 0;
          dVar19 = (double)___sincos_stret(auVar55);
          dVar38 = dVar19 * 1.0;
          dVar36 = dVar19 * 0.0;
          dVar19 = dVar19 * 0.0;
          dVar40 = (((-(-(dVar37 * 0.0) * dVar60) - dVar65) * dVar38 + dVar39 * dVar64) -
                   dVar62 * dVar36) - dVar19 * dVar61;
          dVar41 = (dVar67 * dVar39 + dVar64 * dVar38 + dVar19 * dVar62) - dVar61 * dVar36;
          dVar59 = (dVar62 * dVar39 + dVar64 * dVar36 + dVar61 * dVar38) - dVar19 * dVar67;
          dVar19 = (dVar61 * dVar39 + dVar19 * dVar64 + dVar67 * dVar36) - dVar62 * dVar38;
          dVar37 = ((dVar40 - dVar41 * 0.0) - dVar59 * 0.0) - dVar19 * 0.0;
          dVar38 = (dVar41 + dVar40 * 0.0 + dVar59 * 0.0) - dVar19 * 0.0;
          dVar60 = (dVar59 + dVar40 * 0.0 + dVar19 * 0.0) - dVar41 * 0.0;
          dVar19 = (dVar19 + dVar40 * 0.0 + dVar41 * 0.0) - dVar59 * 0.0;
          dVar36 = dVar60 + dVar60;
          dVar40 = dVar19 + dVar19;
          dVar41 = dVar37 * (dVar38 + dVar38);
          dVar39 = dVar38 * (dVar38 + dVar38);
          *(double *)(lVar27 + -0x58) = dVar38 * dVar36 + dVar37 * dVar40;
          *(double *)(lVar27 + -0x60) = 1.0 - (dVar60 * dVar36 + dVar19 * dVar40);
          *(double *)(puVar26 + -0x12) = dVar38 * dVar36 - dVar37 * dVar40;
          *(double *)(puVar26 + -0x14) = dVar38 * dVar40 - dVar37 * dVar36;
          *(double *)(puVar26 + -0xe) = dVar60 * dVar40 + dVar41;
          *(double *)(puVar26 + -0x10) = 1.0 - (dVar39 + dVar19 * dVar40);
          *(double *)(puVar26 + -10) = dVar60 * dVar40 - dVar41;
          *(double *)(puVar26 + -0xc) = dVar38 * dVar40 + dVar37 * dVar36;
          *(double *)(puVar26 + -8) = 1.0 - (dVar39 + dVar60 * dVar36);
          *puVar26 = local_230;
          iVar8 = iVar8 + 1;
          lVar29 = lVar29 + 0x288;
          lVar31 = lVar31 + 0xc0;
        } while ((ulong)uVar7 * 0x288 != lVar29);
LAB_1027a76ac:
        lVar29 = param_2[1];
      }
LAB_1027a76b0:
      if ((lVar29 != 2) || (lVar29 = *param_2, *(int *)(lVar29 + 0xb0) != 5)) goto LAB_1027a76d8;
LAB_1027a76c8:
      uVar28 = 0;
      *(undefined4 *)(lVar29 + 0xb4) = 1;
    }
    else {
      if (iVar8 == 3) {
        if (local_128[0x17] < '\0') {
          pcVar14 = *(char **)local_128;
        }
        uVar7 = _atoi(pcVar14);
        if (uVar7 == 0) goto LAB_1027a5130;
        lVar29 = (long)(int)uVar7;
        lVar31 = param_2[1];
        if (lVar31 != lVar29) {
          pvVar30 = (void *)*param_2;
          if ((lVar31 != 0) && (pvVar30 != (void *)0x0)) {
            puVar20 = (undefined8 *)((long)pvVar30 + lVar31 * 0xc0);
            do {
              puVar20 = puVar20 + -0x18;
              lVar31 = lVar31 + -1;
              _free((void *)*puVar20);
            } while (lVar31 != 0);
          }
          _free(pvVar30);
          if ((int)uVar7 < 1) {
            lVar31 = 0;
          }
          else {
            lVar31 = FUN_1027a3950(lVar29);
          }
          *param_2 = lVar31;
          pcVar17 = local_120;
        }
        param_2[1] = lVar29;
        uVar9 = _atoi(pcVar17 + -0x18);
        if (uVar7 * 0x13 + 2 == (int)((ulong)((long)pcVar17 - (long)local_128) >> 3) * -0x55555555)
        {
          if (0 < (int)uVar7) {
            lVar29 = 0;
            iVar8 = 0;
            local_1b0._0_4_ = uVar9 >> 8 & 0xff;
            lVar31 = 0xb0;
            do {
              pcVar17 = local_128;
              pcVar14 = local_128 + lVar29 + 0x18;
              if (local_128[lVar29 + 0x2f] < '\0') {
                pcVar14 = *(char **)pcVar14;
              }
              lVar25 = *param_2;
              lVar27 = lVar25 + lVar31;
              dVar19 = _atof(pcVar14);
              *(double *)(lVar27 + -0x70) = dVar19;
              pcVar14 = pcVar17 + lVar29 + 0x30;
              if (pcVar17[lVar29 + 0x47] < '\0') {
                pcVar14 = *(char **)pcVar14;
              }
              dVar19 = _atof(pcVar14);
              lVar1 = lVar25 + lVar31;
              *(double *)(lVar1 + -0x98) = dVar19;
              if (pcVar17[lVar29 + 0x5f] < '\0') {
                dVar19 = _atof(*(char **)(pcVar17 + lVar29 + 0x48));
                *(double *)(lVar1 + -0x90) = dVar19;
                cVar5 = pcVar17[lVar29 + 0x77];
              }
              else {
                dVar19 = _atof(pcVar17 + lVar29 + 0x48);
                *(double *)(lVar1 + -0x90) = dVar19;
                cVar5 = pcVar17[lVar29 + 0x77];
              }
              pcVar14 = pcVar17 + lVar29 + 0x60;
              if (cVar5 < '\0') {
                pcVar14 = *(char **)(pcVar17 + lVar29 + 0x60);
              }
              dVar19 = _atof(pcVar14);
              lVar1 = lVar25 + lVar31;
              *(double *)(lVar1 + -0x80) = dVar19;
              if (pcVar17[lVar29 + 0x8f] < '\0') {
                dVar19 = _atof(*(char **)(pcVar17 + lVar29 + 0x78));
                *(double *)(lVar1 + -0x78) = dVar19;
                if (-1 < pcVar17[lVar29 + 0xa7]) goto LAB_1027a6484;
LAB_1027a6694:
                dVar19 = _atof(*(char **)(pcVar17 + lVar29 + 0x90));
                pcVar14 = pcVar17 + lVar29 + 0xa8;
                if (-1 < pcVar17[lVar29 + 0xbf]) goto LAB_1027a649c;
LAB_1027a66b0:
                dVar37 = _atof(*(char **)pcVar14);
                pcVar14 = pcVar17 + lVar29 + 0xc0;
                if (-1 < pcVar17[lVar29 + 0xd7]) goto LAB_1027a64b0;
LAB_1027a66c8:
                dVar38 = _atof(*(char **)pcVar14);
                pcVar14 = pcVar17 + lVar29 + 0xd8;
                if (-1 < pcVar17[lVar29 + 0xef]) goto LAB_1027a64c8;
LAB_1027a66e4:
                local_160 = _atof(*(char **)pcVar14);
                pcVar14 = pcVar17 + lVar29 + 0xf0;
                if (-1 < pcVar17[lVar29 + 0x107]) goto LAB_1027a64dc;
LAB_1027a66fc:
                dVar39 = _atof(*(char **)pcVar14);
                cVar5 = pcVar17[lVar29 + 0x11f];
              }
              else {
                dVar19 = _atof(pcVar17 + lVar29 + 0x78);
                *(double *)(lVar1 + -0x78) = dVar19;
                if (pcVar17[lVar29 + 0xa7] < '\0') goto LAB_1027a6694;
LAB_1027a6484:
                dVar19 = _atof(pcVar17 + lVar29 + 0x90);
                pcVar14 = pcVar17 + lVar29 + 0xa8;
                if (pcVar17[lVar29 + 0xbf] < '\0') goto LAB_1027a66b0;
LAB_1027a649c:
                dVar37 = _atof(pcVar14);
                pcVar14 = pcVar17 + lVar29 + 0xc0;
                if (pcVar17[lVar29 + 0xd7] < '\0') goto LAB_1027a66c8;
LAB_1027a64b0:
                dVar38 = _atof(pcVar14);
                pcVar14 = pcVar17 + lVar29 + 0xd8;
                if (pcVar17[lVar29 + 0xef] < '\0') goto LAB_1027a66e4;
LAB_1027a64c8:
                local_160 = _atof(pcVar14);
                pcVar14 = pcVar17 + lVar29 + 0xf0;
                if (pcVar17[lVar29 + 0x107] < '\0') goto LAB_1027a66fc;
LAB_1027a64dc:
                dVar39 = _atof(pcVar14);
                cVar5 = pcVar17[lVar29 + 0x11f];
              }
              pcVar17 = pcVar17 + lVar29 + 0x108;
              if (cVar5 < '\0') {
                pcVar17 = *(char **)pcVar17;
              }
              dVar36 = _atof(pcVar17);
              pcVar14 = local_128;
              lVar2 = lVar25 + lVar31;
              *(double *)(lVar2 + -0x10) = dVar39;
              *(double *)(lVar2 + -0x18) = local_160;
              *(double *)(lVar2 + -8) = dVar36;
              if (local_128[lVar29 + 0x137] < '\0') {
                local_190 = _atof(*(char **)(local_128 + lVar29 + 0x120));
                if (-1 < pcVar14[lVar29 + 0x14f]) goto LAB_1027a6534;
LAB_1027a6738:
                local_160 = _atof(*(char **)(pcVar14 + lVar29 + 0x138));
                pcVar17 = pcVar14 + lVar29 + 0x150;
                if (-1 < pcVar14[lVar29 + 0x167]) goto LAB_1027a654c;
LAB_1027a6754:
                local_170 = _atof(*(char **)pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x168;
                if (-1 < pcVar14[lVar29 + 0x17f]) goto LAB_1027a6560;
LAB_1027a676c:
                dVar39 = _atof(*(char **)pcVar17);
                cVar5 = pcVar14[lVar29 + 0x197];
              }
              else {
                local_190 = _atof(local_128 + lVar29 + 0x120);
                if (pcVar14[lVar29 + 0x14f] < '\0') goto LAB_1027a6738;
LAB_1027a6534:
                local_160 = _atof(pcVar14 + lVar29 + 0x138);
                pcVar17 = pcVar14 + lVar29 + 0x150;
                if (pcVar14[lVar29 + 0x167] < '\0') goto LAB_1027a6754;
LAB_1027a654c:
                local_170 = _atof(pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x168;
                if (pcVar14[lVar29 + 0x17f] < '\0') goto LAB_1027a676c;
LAB_1027a6560:
                dVar39 = _atof(pcVar17);
                cVar5 = pcVar14[lVar29 + 0x197];
              }
              pcVar14 = pcVar14 + lVar29 + 0x180;
              if (cVar5 < '\0') {
                pcVar14 = *(char **)pcVar14;
              }
              dVar36 = _atof(pcVar14);
              pdVar15 = *(double **)(lVar27 + -0xb0);
              if (*(long *)(lVar25 + lVar31 + -0xa8) != 5) {
                _free(pdVar15);
                pdVar15 = _malloc(0x28);
                if (pdVar15 == (double *)0x0) goto LAB_1027a7780;
                *(double **)(lVar27 + -0xb0) = pdVar15;
                *(undefined8 *)(lVar25 + lVar31 + -0xa8) = 5;
              }
              pcVar17 = local_128;
              pdVar15[1] = local_160;
              *pdVar15 = local_190;
              pdVar15[3] = dVar39;
              pdVar15[2] = local_170;
              pdVar15[4] = dVar36;
              pcVar14 = local_128 + lVar29 + 0x198;
              if (local_128[lVar29 + 0x1af] < '\0') {
                pcVar14 = *(char **)pcVar14;
              }
              iVar10 = _atoi(pcVar14);
              lVar2 = lVar25 + lVar31;
              *(int *)(lVar2 + -0xa0) = iVar10;
              if (pcVar17[lVar29 + 0x1c7] < '\0') {
                iVar11 = _atoi(*(char **)(pcVar17 + lVar29 + 0x1b0));
                *(int *)(lVar2 + -0x9c) = iVar11;
                cVar5 = pcVar17[lVar29 + 0x1df];
              }
              else {
                iVar11 = _atoi(pcVar17 + lVar29 + 0x1b0);
                *(int *)(lVar2 + -0x9c) = iVar11;
                cVar5 = pcVar17[lVar29 + 0x1df];
              }
              pcVar17 = pcVar17 + lVar29 + 0x1c8;
              if (cVar5 < '\0') {
                pcVar17 = *(char **)pcVar17;
              }
              uVar9 = _atoi(pcVar17);
              puVar26 = (uint *)(lVar25 + lVar31);
              puVar26[-0x1a] = uVar9;
              puVar26[-0x19] = 3;
              if (uVar7 == 2) {
                if (iVar10 < 0) {
                  iVar10 = iVar10 + 1;
                }
                *(int *)(lVar2 + -0xa0) = iVar10 >> 1;
                *(double *)(lVar1 + -0x80) =
                     *(double *)(lVar1 + -0x80) - (double)((iVar10 >> 1) * iVar8);
              }
              dVar41 = (dVar37 * 3.141592653589793) / 180.0 + 1.5707963267948966;
              auVar51._0_8_ = dVar41 * 0.5;
              auVar51._8_8_ = 0;
              dVar37 = (double)___sincos_stret(auVar51);
              dVar39 = dVar37 * 0.0;
              dVar40 = dVar37 * 1.0;
              dVar61 = dVar37 * 0.0;
              auVar52._0_8_ = ((dVar19 * 3.141592653589793) / 180.0) * 0.5;
              auVar52._8_8_ = 0;
              dVar19 = dVar41;
              dVar36 = (double)___sincos_stret(auVar52);
              dVar59 = dVar36 * 0.0;
              dVar60 = dVar36 * 0.0;
              dVar64 = ((-dVar39 * dVar59 + dVar19 * dVar41) - dVar60 * dVar40) - dVar36 * dVar61;
              dVar65 = dVar19 * dVar39 + dVar41 * dVar59 + dVar36 * dVar40;
              dVar67 = dVar65 - dVar61 * dVar60;
              dVar62 = (dVar19 * dVar40 + dVar41 * dVar60 + dVar61 * dVar59) - dVar36 * dVar39;
              dVar61 = (dVar19 * dVar61 + dVar36 * dVar41 + dVar60 * dVar39) - dVar59 * dVar40;
              dVar39 = 0.5;
              auVar53._0_8_ = ((dVar38 * 3.141592653589793) / 180.0) * 0.5;
              auVar53._8_8_ = 0;
              dVar19 = (double)___sincos_stret(auVar53);
              dVar38 = dVar19 * 1.0;
              dVar36 = dVar19 * 0.0;
              dVar19 = dVar19 * 0.0;
              dVar40 = (((-(-(dVar37 * 0.0) * dVar60) - dVar65) * dVar38 + dVar39 * dVar64) -
                       dVar62 * dVar36) - dVar19 * dVar61;
              dVar41 = (dVar67 * dVar39 + dVar64 * dVar38 + dVar19 * dVar62) - dVar61 * dVar36;
              dVar59 = (dVar62 * dVar39 + dVar64 * dVar36 + dVar61 * dVar38) - dVar19 * dVar67;
              dVar19 = (dVar61 * dVar39 + dVar19 * dVar64 + dVar67 * dVar36) - dVar62 * dVar38;
              dVar37 = ((dVar40 - dVar41 * 0.0) - dVar59 * 0.0) - dVar19 * 0.0;
              dVar38 = (dVar41 + dVar40 * 0.0 + dVar59 * 0.0) - dVar19 * 0.0;
              dVar60 = (dVar59 + dVar40 * 0.0 + dVar19 * 0.0) - dVar41 * 0.0;
              dVar19 = (dVar19 + dVar40 * 0.0 + dVar41 * 0.0) - dVar59 * 0.0;
              dVar36 = dVar60 + dVar60;
              dVar40 = dVar19 + dVar19;
              dVar41 = dVar37 * (dVar38 + dVar38);
              dVar39 = dVar38 * (dVar38 + dVar38);
              *(double *)(lVar27 + -0x58) = dVar38 * dVar36 + dVar37 * dVar40;
              *(double *)(lVar27 + -0x60) = 1.0 - (dVar60 * dVar36 + dVar19 * dVar40);
              *(double *)(puVar26 + -0x12) = dVar38 * dVar36 - dVar37 * dVar40;
              *(double *)(puVar26 + -0x14) = dVar38 * dVar40 - dVar37 * dVar36;
              *(double *)(puVar26 + -0xe) = dVar60 * dVar40 + dVar41;
              *(double *)(puVar26 + -0x10) = 1.0 - (dVar39 + dVar19 * dVar40);
              *(double *)(puVar26 + -10) = dVar60 * dVar40 - dVar41;
              *(double *)(puVar26 + -0xc) = dVar38 * dVar40 + dVar37 * dVar36;
              *(double *)(puVar26 + -8) = 1.0 - (dVar39 + dVar60 * dVar36);
              *puVar26 = local_1b0._0_4_;
              iVar8 = iVar8 + 1;
              lVar29 = lVar29 + 0x1c8;
              lVar31 = lVar31 + 0xc0;
            } while ((ulong)uVar7 * 0x1c8 != lVar29);
            goto LAB_1027a76ac;
          }
          goto LAB_1027a76b0;
        }
      }
      else if (iVar8 == 2) {
        if (local_128[0x17] < '\0') {
          pcVar14 = *(char **)local_128;
        }
        uVar7 = _atoi(pcVar14);
        if (uVar7 == 0) goto LAB_1027a5130;
        lVar29 = (long)(int)uVar7;
        lVar31 = param_2[1];
        if (lVar31 != lVar29) {
          pvVar30 = (void *)*param_2;
          if ((lVar31 != 0) && (pvVar30 != (void *)0x0)) {
            puVar20 = (undefined8 *)((long)pvVar30 + lVar31 * 0xc0);
            do {
              puVar20 = puVar20 + -0x18;
              lVar31 = lVar31 + -1;
              _free((void *)*puVar20);
            } while (lVar31 != 0);
          }
          _free(pvVar30);
          if ((int)uVar7 < 1) {
            lVar31 = 0;
          }
          else {
            lVar31 = FUN_1027a3950(lVar29);
          }
          *param_2 = lVar31;
          pcVar17 = local_120;
        }
        param_2[1] = lVar29;
        uVar9 = _atoi(pcVar17 + -0x18);
        if ((uVar7 << 4 | 2) == (int)((ulong)((long)pcVar17 - (long)local_128) >> 3) * -0x55555555)
        {
          if (0 < (int)uVar7) {
            lVar31 = 0;
            lVar29 = 0;
            iVar8 = 0;
            local_1f0._0_4_ = uVar9 >> 8 & 0xff;
            uVar35 = 0x10;
            do {
              pcVar14 = local_128;
              if (local_128[lVar29 + 0x2f] < '\0') {
                lVar27 = *param_2;
                local_1a0 = _atof(*(char **)(local_128 + lVar29 + 0x18));
                if (-1 < pcVar14[lVar29 + 0x47]) goto LAB_1027a7190;
LAB_1027a7574:
                local_170 = _atof(*(char **)(pcVar14 + lVar29 + 0x30));
                pcVar17 = pcVar14 + lVar29 + 0x48;
                if (-1 < pcVar14[lVar29 + 0x5f]) goto LAB_1027a71a8;
LAB_1027a7590:
                local_160 = _atof(*(char **)pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x60;
                if (-1 < pcVar14[lVar29 + 0x77]) goto LAB_1027a71bc;
LAB_1027a75a8:
                dVar19 = _atof(*(char **)pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x78;
                if (-1 < pcVar14[lVar29 + 0x8f]) goto LAB_1027a71d4;
LAB_1027a75c4:
                dVar37 = _atof(*(char **)pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x90;
                if (-1 < pcVar14[lVar29 + 0xa7]) goto LAB_1027a71e8;
LAB_1027a75dc:
                dVar38 = _atof(*(char **)pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0xa8;
                if (-1 < pcVar14[lVar29 + 0xbf]) goto LAB_1027a7200;
LAB_1027a75f8:
                local_180 = _atof(*(char **)pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0xc0;
                if (-1 < pcVar14[lVar29 + 0xd7]) goto LAB_1027a7214;
LAB_1027a7610:
                dVar39 = _atof(*(char **)pcVar17);
                cVar5 = pcVar14[lVar29 + 0xef];
                dVar40 = local_170;
                dVar36 = local_160;
              }
              else {
                lVar27 = *param_2;
                local_1a0 = _atof(local_128 + lVar29 + 0x18);
                if (pcVar14[lVar29 + 0x47] < '\0') goto LAB_1027a7574;
LAB_1027a7190:
                local_170 = _atof(pcVar14 + lVar29 + 0x30);
                pcVar17 = pcVar14 + lVar29 + 0x48;
                if (pcVar14[lVar29 + 0x5f] < '\0') goto LAB_1027a7590;
LAB_1027a71a8:
                local_160 = _atof(pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x60;
                if (pcVar14[lVar29 + 0x77] < '\0') goto LAB_1027a75a8;
LAB_1027a71bc:
                dVar19 = _atof(pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x78;
                if (pcVar14[lVar29 + 0x8f] < '\0') goto LAB_1027a75c4;
LAB_1027a71d4:
                dVar37 = _atof(pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x90;
                if (pcVar14[lVar29 + 0xa7] < '\0') goto LAB_1027a75dc;
LAB_1027a71e8:
                dVar38 = _atof(pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0xa8;
                if (pcVar14[lVar29 + 0xbf] < '\0') goto LAB_1027a75f8;
LAB_1027a7200:
                local_180 = _atof(pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0xc0;
                if (pcVar14[lVar29 + 0xd7] < '\0') goto LAB_1027a7610;
LAB_1027a7214:
                dVar39 = _atof(pcVar17);
                cVar5 = pcVar14[lVar29 + 0xef];
                dVar40 = local_170;
                dVar36 = local_160;
              }
              local_160 = dVar40;
              pcVar14 = pcVar14 + lVar29 + 0xd8;
              if (cVar5 < '\0') {
                pcVar14 = *(char **)pcVar14;
              }
              dVar40 = _atof(pcVar14);
              pcVar14 = local_128;
              lVar25 = lVar27 + lVar31;
              *(double *)(lVar25 + 0xa0) = dVar39;
              *(double *)(lVar25 + 0x98) = local_180;
              *(double *)(lVar25 + 0xa8) = dVar40;
              if (local_128[lVar29 + 0x11f] < '\0') {
                local_1b0 = _atof(*(char **)(local_128 + lVar29 + 0x108));
                if (-1 < pcVar14[lVar29 + 0x137]) goto LAB_1027a726c;
LAB_1027a764c:
                local_1c0 = _atof(*(char **)(pcVar14 + lVar29 + 0x120));
                pcVar17 = pcVar14 + lVar29 + 0x138;
                if (-1 < pcVar14[lVar29 + 0x14f]) goto LAB_1027a7284;
LAB_1027a7668:
                local_1d0 = _atof(*(char **)pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x150;
                if (-1 < pcVar14[lVar29 + 0x167]) goto LAB_1027a7298;
LAB_1027a7680:
                iVar10 = _atoi(*(char **)pcVar17);
                cVar5 = pcVar14[lVar29 + 0x17f];
              }
              else {
                local_1b0 = _atof(local_128 + lVar29 + 0x108);
                if (pcVar14[lVar29 + 0x137] < '\0') goto LAB_1027a764c;
LAB_1027a726c:
                local_1c0 = _atof(pcVar14 + lVar29 + 0x120);
                pcVar17 = pcVar14 + lVar29 + 0x138;
                if (pcVar14[lVar29 + 0x14f] < '\0') goto LAB_1027a7668;
LAB_1027a7284:
                local_1d0 = _atof(pcVar17);
                pcVar17 = pcVar14 + lVar29 + 0x150;
                if (pcVar14[lVar29 + 0x167] < '\0') goto LAB_1027a7680;
LAB_1027a7298:
                iVar10 = _atoi(pcVar17);
                cVar5 = pcVar14[lVar29 + 0x17f];
              }
              pcVar17 = pcVar14 + lVar29 + 0x168;
              if (cVar5 < '\0') {
                pcVar17 = *(char **)pcVar17;
              }
              iVar11 = _atoi(pcVar17);
              if (uVar7 == 2) {
                if (iVar10 < 0) {
                  iVar10 = iVar10 + 1;
                }
                iVar10 = iVar10 >> 1;
                local_160 = local_160 - (double)(iVar10 * iVar8);
              }
              pcVar14 = pcVar14 + (uVar35 & 0xfffffff0) * 0x18;
              if (pcVar14[0x17] < '\0') {
                pcVar14 = *(char **)pcVar14;
              }
              puVar20 = (undefined8 *)(lVar27 + lVar31);
              iVar12 = _atoi(pcVar14);
              dVar39 = (double)FUN_1027aa2d8();
              dVar60 = (dVar37 * 3.141592653589793) / 180.0 + 1.5707963267948966;
              auVar56._0_8_ = dVar60 * 0.5;
              auVar56._8_8_ = 0;
              dVar37 = (double)___sincos_stret(auVar56);
              dVar40 = dVar37 * 0.0;
              dVar59 = dVar37 * 1.0;
              dVar67 = dVar37 * 0.0;
              auVar57._0_8_ = ((dVar19 * 3.141592653589793) / 180.0) * 0.5;
              auVar57._8_8_ = 0;
              dVar19 = dVar60;
              dVar41 = (double)___sincos_stret(auVar57);
              dVar61 = dVar41 * 0.0;
              dVar62 = dVar41 * 0.0;
              dVar64 = ((-dVar40 * dVar61 + dVar19 * dVar60) - dVar62 * dVar59) - dVar41 * dVar67;
              dVar66 = dVar19 * dVar40 + dVar60 * dVar61 + dVar41 * dVar59;
              dVar68 = dVar66 - dVar67 * dVar62;
              dVar65 = (dVar19 * dVar59 + dVar60 * dVar62 + dVar67 * dVar61) - dVar41 * dVar40;
              dVar19 = dVar19 * dVar67;
              dVar67 = (dVar19 + dVar41 * dVar60 + dVar62 * dVar40) - dVar61 * dVar59;
              auVar58._0_8_ = ((dVar38 * 3.141592653589793) / 180.0) * 0.5;
              auVar58._8_8_ = 0;
              dVar38 = (double)___sincos_stret(auVar58);
              dVar40 = dVar38 * 1.0;
              dVar41 = dVar38 * 0.0;
              dVar38 = dVar38 * 0.0;
              dVar59 = (((-(-(dVar37 * 0.0) * dVar62) - dVar66) * dVar40 + dVar19 * dVar64) -
                       dVar65 * dVar41) - dVar38 * dVar67;
              dVar60 = (dVar68 * dVar19 + dVar64 * dVar40 + dVar38 * dVar65) - dVar67 * dVar41;
              dVar61 = (dVar65 * dVar19 + dVar64 * dVar41 + dVar67 * dVar40) - dVar38 * dVar68;
              dVar19 = (dVar67 * dVar19 + dVar38 * dVar64 + dVar68 * dVar41) - dVar65 * dVar40;
              dVar37 = ((dVar59 - dVar60 * 0.0) - dVar61 * 0.0) - dVar19 * 0.0;
              dVar38 = (dVar60 + dVar59 * 0.0 + dVar61 * 0.0) - dVar19 * 0.0;
              dVar62 = (dVar61 + dVar59 * 0.0 + dVar19 * 0.0) - dVar60 * 0.0;
              dVar19 = (dVar19 + dVar59 * 0.0 + dVar60 * 0.0) - dVar61 * 0.0;
              dVar41 = dVar62 + dVar62;
              dVar59 = dVar19 + dVar19;
              dVar60 = dVar37 * (dVar38 + dVar38);
              dVar40 = dVar38 * (dVar38 + dVar38);
              puVar20[0xb] = dVar38 * dVar41 + dVar37 * dVar59;
              puVar20[10] = 1.0 - (dVar62 * dVar41 + dVar19 * dVar59);
              puVar3 = (undefined8 *)(lVar27 + lVar31);
              puVar3[0xd] = dVar38 * dVar41 - dVar37 * dVar59;
              puVar3[0xc] = dVar38 * dVar59 - dVar37 * dVar41;
              puVar3[0xf] = dVar62 * dVar59 + dVar60;
              puVar3[0xe] = 1.0 - (dVar40 + dVar19 * dVar59);
              puVar3[0x11] = dVar62 * dVar59 - dVar60;
              puVar3[0x10] = dVar38 * dVar59 + dVar37 * dVar41;
              puVar3[0x12] = 1.0 - (dVar40 + dVar62 * dVar41);
              *(uint *)(puVar3 + 0x16) = local_1f0._0_4_;
              puVar16 = (undefined8 *)*puVar3;
              if (puVar3[1] != 4) {
                _free(puVar16);
                puVar16 = _malloc(0x20);
                if (puVar16 == (undefined8 *)0x0) {
LAB_1027a7780:
                  ___cxa_allocate_exception(8);
                  uVar28 = __ZNSt9bad_allocC1Ev();
                    /* WARNING: Subroutine does not return */
                  ___cxa_throw(uVar28,PTR___ZTISt9bad_alloc_103fec370,
                               PTR___ZNSt9bad_allocD1Ev_103fec1c0);
                }
                *puVar20 = puVar16;
                puVar3[1] = 4;
              }
              auVar63 = NEON_fmov(0x3ff0000000000000,8);
              puVar16[1] = local_1b0 * 0.017453292519943295 * 57.29577951308232;
              *puVar16 = auVar63._0_8_;
              puVar16[3] = local_1d0 * 5.316576934207788e-06 * 188090.94881441945;
              puVar16[2] = local_1c0 * 0.00030461741978670857 * 3282.806350011744;
              dVar39 = dVar39 * 0.5;
              dVar19 = (local_1a0 * 180.0) /
                       (dVar39 * (dVar39 * (local_1b0 * 0.017453292519943295 +
                                           dVar39 * (local_1c0 * 0.00030461741978670857 +
                                                    dVar39 * local_1d0 * 5.316576934207788e-06)) +
                                 1.0) * 3.141592653589793);
              puVar3[3] = dVar19;
              puVar3[4] = dVar19;
              puVar3[7] = dVar36;
              puVar3[6] = local_160;
              *(int *)(puVar3 + 2) = iVar10;
              *(int *)((long)puVar3 + 0x14) = iVar11;
              *(int *)(puVar3 + 9) = iVar12;
              *(undefined4 *)((long)puVar3 + 0x4c) = 0;
              iVar8 = iVar8 + 1;
              lVar29 = lVar29 + 0x180;
              uVar35 = uVar35 + 0x10;
              lVar31 = lVar31 + 0xc0;
            } while ((ulong)uVar7 * 0x180 != lVar29);
            goto LAB_1027a76ac;
          }
          goto LAB_1027a76b0;
        }
      }
LAB_1027a76a4:
      uVar28 = 0xffffffff;
    }
  }
LAB_1027a76dc:
  if ((char)local_129 < '\0') {
    __ZdlPv(CONCAT62(uStack_13e,local_140));
  }
  if (local_128 != (char *)0x0) {
    for (; local_120 != local_128; local_120 = local_120 + -0x18) {
    }
    local_120 = local_128;
    __ZdlPv(local_128);
  }
LAB_1027a7734:
  if (*(long *)PTR____stack_chk_guard_103fed798 != local_b0) {
                    /* WARNING: Subroutine does not return */
    ___stack_chk_fail();
  }
  return uVar28;
}

