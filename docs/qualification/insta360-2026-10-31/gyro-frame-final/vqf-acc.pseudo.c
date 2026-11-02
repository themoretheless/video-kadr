
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(double *param_1,double *param_2)

{
  bool bVar1;
  bool bVar2;
  double *extraout_x1;
  undefined1 uVar3;
  uint unaff_w20;
  char cVar4;
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
  double dVar20;
  double dVar21;
  double dVar22;
  double dVar23;
  double dVar24;
  double dVar25;
  double dVar26;
  double extraout_d16;
  double dVar27;
  double dVar28;
  double dVar29;
  double extraout_d17;
  double dVar30;
  double dVar31;
  double dVar32;
  double dVar33;
  double dVar34;
  double dVar35;
  double dVar36;
  double dVar37;
  double dVar38;
  double dVar39;
  double dVar40;
  double dVar41;
  double dVar42;
  double dVar43;
  double dVar44;
  double dVar45;
  double dVar46;
  double dVar47;
  double dVar48;
  double dVar49;
  double local_98;
  double local_90;
  double local_88;
  double dStack_80;
  long local_78;
  
  local_78 = *_DAT_103fed798;
  if (((*param_2 == 0.0) && (param_2[1] == 0.0)) && (param_2[2] == 0.0)) goto LAB_1027d38ec;
  local_98 = param_1[0x73];
  dVar5 = local_98 + param_1[0xa0];
  param_1[0xa0] = dVar5;
  unaff_w20 = (uint)*(byte *)((long)param_1 + 0x11);
  if (*(byte *)((long)param_1 + 0x11) == 0) goto LAB_1027d2a88;
  if (!NAN(param_1[0x5f])) {
    dVar7 = *param_2;
    dVar8 = param_1[0x85];
    dVar6 = param_1[0x5f] + dVar7 * dVar8;
    dVar11 = param_1[0x86];
    dVar13 = param_1[0x88];
    param_1[0x5f] = param_1[0x60] + -(dVar13 * dVar6) + dVar7 * dVar11;
    dVar14 = param_1[0x87];
    dVar15 = param_1[0x89];
    param_1[0x60] = -(dVar15 * dVar6) + dVar7 * dVar14;
    param_1[0x5c] = dVar6;
    dVar23 = param_2[1];
    dVar7 = param_1[0x61] + dVar23 * dVar8;
    param_1[0x61] = param_1[0x62] + -(dVar13 * dVar7) + dVar23 * dVar11;
    param_1[0x62] = -(dVar15 * dVar7) + dVar23 * dVar14;
    param_1[0x5d] = dVar7;
    dVar23 = param_2[2];
    dVar8 = param_1[99] + dVar23 * dVar8;
    param_1[99] = param_1[100] + -(dVar13 * dVar8) + dVar23 * dVar11;
    param_1[100] = -(dVar15 * dVar8) + dVar23 * dVar14;
    param_1[0x5e] = dVar8;
    goto LAB_1027d2a20;
  }
  do {
    if (NAN(param_1[0x60])) {
      param_1[0x62] = 0.0;
      param_1[0x61] = 0.0;
      dVar11 = 1.0;
      dVar8 = 0.0;
      dVar7 = 0.0;
      dVar6 = 0.0;
      param_1[99] = 0.0;
    }
    else {
      dVar11 = param_1[0x60] + 1.0;
      dVar6 = param_1[0x61];
      dVar7 = param_1[0x62];
      dVar8 = param_1[99];
    }
    param_1[0x60] = dVar11;
    dVar13 = *param_2;
    param_1[0x61] = dVar13 + dVar6;
    dVar6 = (dVar13 + dVar6) / dVar11;
    param_1[0x5c] = dVar6;
    dVar13 = param_2[1];
    param_1[0x62] = dVar13 + dVar7;
    dVar7 = (dVar13 + dVar7) / dVar11;
    param_1[0x5d] = dVar7;
    dVar13 = param_2[2];
    param_1[99] = dVar13 + dVar8;
    dVar8 = (dVar13 + dVar8) / dVar11;
    param_1[0x5e] = dVar8;
    if (param_1[10] <= local_98 * dVar11) {
      dVar11 = 1.0 - param_1[0x85];
      param_1[0x5f] = dVar6 * dVar11;
      dVar13 = param_1[0x87] - param_1[0x89];
      param_1[0x60] = dVar6 * dVar13;
      param_1[0x61] = dVar7 * dVar11;
      param_1[0x62] = dVar7 * dVar13;
      param_1[99] = dVar8 * dVar11;
      param_1[100] = dVar8 * dVar13;
    }
LAB_1027d2a20:
    dVar6 = (*param_2 - dVar6) * (*param_2 - dVar6) + (param_2[1] - dVar7) * (param_2[1] - dVar7) +
            (param_2[2] - dVar8) * (param_2[2] - dVar8);
    param_1[0x51] = dVar6;
    if (param_1[0xc] * param_1[0xc] <= dVar6) {
      uVar3 = 0;
      param_1[0x52] = 0.0;
LAB_1027d2a84:
      *(undefined1 *)(param_1 + 0x20) = uVar3;
    }
    else {
      dVar6 = param_1[0x52];
      param_1[0x52] = local_98 + dVar6;
      if (param_1[9] <= local_98 + dVar6) {
        uVar3 = 1;
        goto LAB_1027d2a84;
      }
    }
LAB_1027d2a88:
    dVar14 = param_1[0x19];
    dVar15 = param_1[0x1a];
    dVar13 = 1.0;
    dVar7 = *param_2;
    dVar24 = param_2[1];
    dVar19 = param_1[0x17];
    dVar23 = param_1[0x18];
    dVar30 = param_2[2];
    dVar11 = (dVar24 + dVar24) * (-(dVar19 * dVar15) + dVar23 * dVar14) +
             dVar7 * (dVar14 * dVar14 * -2.0 + 1.0 + dVar15 * dVar15 * -2.0) +
             (dVar15 * dVar23 + dVar14 * dVar19) * (dVar30 + dVar30);
    dVar6 = dVar23 * dVar23 * -2.0 + 1.0;
    dVar8 = dVar24 * (dVar6 + dVar15 * dVar15 * -2.0) +
            (dVar14 * dVar23 + dVar15 * dVar19) * (dVar7 + dVar7) +
            (-(dVar23 * dVar19) + dVar15 * dVar14) * (dVar30 + dVar30);
    dVar6 = (dVar24 + dVar24) * (dVar14 * dVar15 + dVar23 * dVar19) +
            (-(dVar19 * dVar14) + dVar23 * dVar15) * (dVar7 + dVar7) +
            (dVar6 + dVar14 * dVar14 * -2.0) * dVar30;
    dVar7 = *param_1;
    if (NAN(param_1[0x24])) {
      if (NAN(param_1[0x25])) {
        dVar16 = 0.0;
        dVar9 = 0.0;
        dVar24 = 0.0;
      }
      else {
        dVar13 = param_1[0x25] + 1.0;
        dVar24 = param_1[0x26];
        dVar9 = param_1[0x27];
        dVar16 = param_1[0x28];
      }
      param_1[0x25] = dVar13;
      param_1[0x26] = dVar11 + dVar24;
      dVar24 = (dVar11 + dVar24) / dVar13;
      dVar30 = (dVar8 + dVar9) / dVar13;
      param_1[0x21] = dVar24;
      param_1[0x22] = dVar30;
      param_1[0x27] = dVar8 + dVar9;
      param_1[0x28] = dVar6 + dVar16;
      dVar9 = (dVar6 + dVar16) / dVar13;
      param_1[0x23] = dVar9;
      if (dVar7 <= local_98 * dVar13) {
        dVar13 = 1.0 - param_1[0x75];
        dVar16 = param_1[0x77] - param_1[0x79];
        param_1[0x24] = dVar24 * dVar13;
        param_1[0x25] = dVar24 * dVar16;
        param_1[0x26] = dVar30 * dVar13;
        param_1[0x27] = dVar30 * dVar16;
        param_1[0x28] = dVar9 * dVar13;
        param_1[0x29] = dVar9 * dVar16;
      }
      dVar13 = param_1[0x93];
      if (NAN(dVar13)) goto LAB_1027d2d14;
LAB_1027d2bbc:
      dVar16 = param_1[0x99];
      dVar13 = dVar13 + dVar11 * dVar16;
      dVar25 = param_1[0x9a];
      dVar27 = param_1[0x9c];
      param_1[0x93] = param_1[0x94] + -(dVar27 * dVar13) + dVar11 * dVar25;
      dVar32 = param_1[0x9b];
      dVar35 = param_1[0x9d];
      param_1[0x94] = -(dVar35 * dVar13) + dVar11 * dVar32;
      param_1[0x90] = dVar13;
      dVar11 = param_1[0x95] + dVar8 * dVar16;
      param_1[0x95] = param_1[0x96] + -(dVar27 * dVar11) + dVar8 * dVar25;
      param_1[0x96] = -(dVar35 * dVar11) + dVar8 * dVar32;
      param_1[0x91] = dVar11;
      dVar8 = param_1[0x97] + dVar6 * dVar16;
      param_1[0x97] = param_1[0x98] + -(dVar27 * dVar8) + dVar6 * dVar25;
      param_1[0x98] = -(dVar35 * dVar8) + dVar6 * dVar32;
      param_1[0x92] = dVar8;
      cVar4 = *(char *)(param_1 + 0x20);
      if (cVar4 != '\0') goto LAB_1027d2c58;
LAB_1027d2dd4:
      if (dVar7 + 1.0 < dVar5) {
        if (100.0 < dVar11 * dVar11 + dVar13 * dVar13 + dVar8 * dVar8) {
          param_1[0x9e] = 0.0;
        }
        else {
          dVar5 = param_1[0x9e];
          param_1[0x9e] = local_98 + dVar5;
          if (3.0 <= local_98 + dVar5) goto LAB_1027d2c60;
        }
      }
      *(undefined4 *)(param_1 + 0x9f) = 0;
    }
    else {
      dVar13 = param_1[0x75];
      dVar24 = param_1[0x24] + dVar11 * dVar13;
      dVar16 = param_1[0x76];
      dVar25 = param_1[0x78];
      dVar27 = param_1[0x25];
      dVar32 = param_1[0x77];
      dVar35 = param_1[0x79];
      dVar30 = param_1[0x26] + dVar8 * dVar13;
      param_1[0x25] = -(dVar35 * dVar24) + dVar11 * dVar32;
      param_1[0x26] = param_1[0x27] + -(dVar25 * dVar30) + dVar8 * dVar16;
      dVar9 = param_1[0x28] + dVar6 * dVar13;
      param_1[0x21] = dVar24;
      param_1[0x22] = dVar30;
      param_1[0x27] = -(dVar35 * dVar30) + dVar8 * dVar32;
      param_1[0x28] = param_1[0x29] + -(dVar25 * dVar9) + dVar6 * dVar16;
      param_1[0x29] = -(dVar35 * dVar9) + dVar6 * dVar32;
      param_1[0x23] = dVar9;
      param_1[0x24] = dVar27 + -(dVar25 * dVar24) + dVar11 * dVar16;
      dVar13 = param_1[0x93];
      if (!NAN(dVar13)) goto LAB_1027d2bbc;
LAB_1027d2d14:
      dVar16 = 1.0;
      if (NAN(param_1[0x94])) {
        dVar25 = 0.0;
        dVar27 = 0.0;
        dVar13 = 0.0;
      }
      else {
        dVar16 = param_1[0x94] + 1.0;
        dVar13 = param_1[0x95];
        dVar27 = param_1[0x96];
        dVar25 = param_1[0x97];
      }
      param_1[0x94] = dVar16;
      param_1[0x95] = dVar11 + dVar13;
      dVar13 = (dVar11 + dVar13) / dVar16;
      param_1[0x90] = dVar13;
      param_1[0x96] = dVar8 + dVar27;
      dVar11 = (dVar8 + dVar27) / dVar16;
      param_1[0x91] = dVar11;
      param_1[0x97] = dVar6 + dVar25;
      dVar8 = (dVar6 + dVar25) / dVar16;
      param_1[0x92] = dVar8;
      if (3.0 <= local_98 * dVar16) {
        dVar6 = 1.0 - param_1[0x99];
        param_1[0x93] = dVar13 * dVar6;
        dVar16 = param_1[0x9b] - param_1[0x9d];
        param_1[0x94] = dVar13 * dVar16;
        param_1[0x95] = dVar11 * dVar6;
        param_1[0x96] = dVar11 * dVar16;
        param_1[0x97] = dVar8 * dVar6;
        param_1[0x98] = dVar8 * dVar16;
      }
      cVar4 = *(char *)(param_1 + 0x20);
      if (cVar4 == '\0') goto LAB_1027d2dd4;
LAB_1027d2c58:
      param_1[0x9e] = 4.0;
LAB_1027d2c60:
      *(undefined4 *)(param_1 + 0x9f) = 1;
    }
    dVar8 = param_1[0x1d];
    dVar11 = param_1[0x1e];
    dVar25 = 1.0;
    dVar16 = param_1[0x1b];
    dVar13 = param_1[0x1c];
    dVar32 = (dVar30 + dVar30) * (-(dVar16 * dVar11) + dVar13 * dVar8) +
             dVar24 * (dVar8 * dVar8 * -2.0 + 1.0 + dVar11 * dVar11 * -2.0) +
             (dVar11 * dVar13 + dVar8 * dVar16) * (dVar9 + dVar9);
    dVar5 = dVar13 * dVar13 * -2.0 + 1.0;
    dVar27 = dVar30 * (dVar5 + dVar11 * dVar11 * -2.0) +
             (dVar8 * dVar13 + dVar11 * dVar16) * (dVar24 + dVar24) +
             (-(dVar13 * dVar16) + dVar11 * dVar8) * (dVar9 + dVar9);
    dVar5 = (dVar30 + dVar30) * (dVar8 * dVar11 + dVar13 * dVar16) +
            (-(dVar16 * dVar8) + dVar13 * dVar11) * (dVar24 + dVar24) +
            (dVar5 + dVar8 * dVar8 * -2.0) * dVar9;
    dVar6 = SQRT(dVar32 * dVar32 + 0.0 + dVar27 * dVar27 + dVar5 * dVar5);
    if (2.220446049250313e-16 <= dVar6) {
      dVar32 = dVar32 / dVar6;
      dVar27 = dVar27 / dVar6;
      dVar5 = dVar5 / dVar6;
    }
    dVar24 = SQRT((dVar5 + 1.0) * 0.5);
    dVar6 = 0.0;
    dVar5 = 0.0;
    if (1e-06 < dVar24) {
      dVar25 = (dVar27 * 0.5) / dVar24;
      dVar6 = (dVar32 * -0.5) / dVar24;
      dVar5 = dVar24;
    }
    dVar30 = ((-(dVar25 * dVar13) + dVar16 * dVar5) - dVar8 * dVar6) + dVar11 * -0.0;
    dVar9 = dVar16 * dVar25 + dVar13 * dVar5 + dVar11 * dVar6 + dVar8 * -0.0;
    dVar24 = -(dVar25 * dVar11) + dVar8 * dVar5 + dVar16 * dVar6 + dVar13 * 0.0;
    dVar6 = ((dVar8 * dVar25 + dVar11 * dVar5) - dVar13 * dVar6) + dVar16 * 0.0;
    param_1[0x1b] = dVar30;
    param_1[0x1c] = dVar9;
    param_1[0x1d] = dVar24;
    param_1[0x1e] = dVar6;
    dVar5 = SQRT(dVar30 * dVar30 + 0.0 + dVar9 * dVar9 + dVar24 * dVar24 + dVar6 * dVar6);
    if (2.220446049250313e-16 <= dVar5) {
      dVar30 = dVar30 / dVar5;
      dVar9 = dVar9 / dVar5;
      param_1[0x1b] = dVar30;
      param_1[0x1c] = dVar9;
      dVar24 = dVar24 / dVar5;
      dVar6 = dVar6 / dVar5;
      param_1[0x1d] = dVar24;
      param_1[0x1e] = dVar6;
    }
    dVar5 = (double)func_0x0001034628c0();
    param_1[0x2a] = dVar5 / local_98;
    if (*(char *)(param_1 + 2) != '\0' || unaff_w20 != 0) {
      dVar11 = ((-(dVar9 * dVar23) + dVar19 * dVar30) - dVar14 * dVar24) - dVar15 * dVar6;
      dVar13 = (dVar19 * dVar9 + dVar23 * dVar30 + dVar15 * dVar24) - dVar14 * dVar6;
      dVar16 = -(dVar9 * dVar15) + dVar14 * dVar30 + dVar19 * dVar24 + dVar23 * dVar6;
      dVar14 = ((dVar9 * dVar14 + dVar15 * dVar30) - dVar23 * dVar24) + dVar19 * dVar6;
      dVar30 = 1.0;
      dVar9 = dVar16 * dVar16 * -2.0 + 1.0 + dVar14 * dVar14 * -2.0;
      dVar5 = -(dVar11 * dVar14) + dVar13 * dVar16;
      dVar5 = dVar5 + dVar5;
      dVar6 = dVar13 * dVar14 + dVar16 * dVar11;
      dVar6 = dVar6 + dVar6;
      dVar23 = dVar13 * dVar16 + dVar14 * dVar11;
      dVar23 = dVar23 + dVar23;
      dVar15 = dVar13 * dVar13 * -2.0 + 1.0;
      dVar24 = dVar15 + dVar14 * dVar14 * -2.0;
      dVar8 = -(dVar13 * dVar11) + dVar14 * dVar16;
      dVar8 = dVar8 + dVar8;
      dVar19 = -(dVar11 * dVar16) + dVar13 * dVar14;
      dVar19 = dVar19 + dVar19;
      dVar11 = dVar14 * dVar16 + dVar13 * dVar11;
      dVar11 = dVar11 + dVar11;
      dVar15 = dVar15 + dVar16 * dVar16 * -2.0;
      if (NAN(param_1[0x3a])) {
        if (NAN(param_1[0x3b])) {
          dVar14 = 0.0;
          dVar13 = 0.0;
          dVar16 = 0.0;
          dVar22 = 0.0;
          dVar21 = 0.0;
          dVar38 = 0.0;
          dVar20 = 0.0;
          dVar43 = 0.0;
          dVar25 = 0.0;
        }
        else {
          dVar30 = param_1[0x3b] + 1.0;
          dVar25 = param_1[0x3c];
          dVar43 = param_1[0x3d];
          dVar20 = param_1[0x3e];
          dVar38 = param_1[0x3f];
          dVar21 = param_1[0x40];
          dVar22 = param_1[0x41];
          dVar16 = param_1[0x42];
          dVar13 = param_1[0x43];
          dVar14 = param_1[0x44];
        }
        param_1[0x3b] = dVar30;
        param_1[0x3c] = dVar9 + dVar25;
        dVar25 = (dVar9 + dVar25) / dVar30;
        dVar35 = (dVar5 + dVar43) / dVar30;
        param_1[0x3d] = dVar5 + dVar43;
        param_1[0x3e] = dVar6 + dVar20;
        dVar43 = (dVar6 + dVar20) / dVar30;
        dVar20 = (dVar23 + dVar38) / dVar30;
        param_1[0x3f] = dVar23 + dVar38;
        param_1[0x40] = dVar24 + dVar21;
        dVar21 = (dVar24 + dVar21) / dVar30;
        param_1[0x41] = dVar8 + dVar22;
        dVar22 = (dVar8 + dVar22) / dVar30;
        param_1[0x42] = dVar19 + dVar16;
        dVar16 = (dVar19 + dVar16) / dVar30;
        param_1[0x43] = dVar11 + dVar13;
        dVar13 = (dVar11 + dVar13) / dVar30;
        param_1[0x44] = dVar15 + dVar14;
        dVar14 = (dVar15 + dVar14) / dVar30;
        dVar11 = local_98;
        if (dVar7 <= local_98 * dVar30) {
          dVar15 = 1.0 - param_1[0x75];
          dVar19 = param_1[0x77] - param_1[0x79];
          param_1[0x3a] = dVar25 * dVar15;
          param_1[0x3b] = dVar25 * dVar19;
          param_1[0x3c] = dVar35 * dVar15;
          param_1[0x3d] = dVar35 * dVar19;
          param_1[0x3e] = dVar43 * dVar15;
          param_1[0x3f] = dVar43 * dVar19;
          param_1[0x40] = dVar20 * dVar15;
          param_1[0x41] = dVar20 * dVar19;
          param_1[0x42] = dVar21 * dVar15;
          param_1[0x43] = dVar21 * dVar19;
          param_1[0x44] = dVar22 * dVar15;
          param_1[0x45] = dVar22 * dVar19;
          param_1[0x46] = dVar16 * dVar15;
          param_1[0x47] = dVar16 * dVar19;
          param_1[0x48] = dVar13 * dVar15;
          param_1[0x49] = dVar13 * dVar19;
          param_1[0x4a] = dVar14 * dVar15;
          param_1[0x4b] = dVar14 * dVar19;
        }
      }
      else {
        dVar7 = param_1[0x75];
        dVar25 = param_1[0x3a] + dVar9 * dVar7;
        dVar14 = param_1[0x76];
        dVar30 = param_1[0x78];
        dVar38 = param_1[0x77];
        dVar40 = param_1[0x79];
        param_1[0x3a] = param_1[0x3b] + -(dVar30 * dVar25) + dVar9 * dVar14;
        param_1[0x3b] = -(dVar40 * dVar25) + dVar9 * dVar38;
        dVar35 = param_1[0x3c] + dVar5 * dVar7;
        param_1[0x3c] = param_1[0x3d] + -(dVar30 * dVar35) + dVar5 * dVar14;
        param_1[0x3d] = -(dVar40 * dVar35) + dVar5 * dVar38;
        dVar43 = param_1[0x3e] + dVar6 * dVar7;
        param_1[0x3e] = param_1[0x3f] + -(dVar30 * dVar43) + dVar6 * dVar14;
        param_1[0x3f] = -(dVar40 * dVar43) + dVar6 * dVar38;
        dVar20 = param_1[0x40] + dVar23 * dVar7;
        param_1[0x40] = param_1[0x41] + -(dVar30 * dVar20) + dVar23 * dVar14;
        param_1[0x41] = -(dVar40 * dVar20) + dVar23 * dVar38;
        dVar21 = param_1[0x42] + dVar24 * dVar7;
        param_1[0x42] = param_1[0x43] + -(dVar30 * dVar21) + dVar24 * dVar14;
        param_1[0x43] = -(dVar40 * dVar21) + dVar24 * dVar38;
        dVar22 = param_1[0x44] + dVar8 * dVar7;
        param_1[0x44] = param_1[0x45] + -(dVar30 * dVar22) + dVar8 * dVar14;
        param_1[0x45] = -(dVar40 * dVar22) + dVar8 * dVar38;
        dVar16 = param_1[0x46] + dVar19 * dVar7;
        param_1[0x46] = param_1[0x47] + -(dVar30 * dVar16) + dVar19 * dVar14;
        param_1[0x47] = -(dVar40 * dVar16) + dVar19 * dVar38;
        dVar13 = param_1[0x48] + dVar11 * dVar7;
        param_1[0x48] = param_1[0x49] + -(dVar30 * dVar13) + dVar11 * dVar14;
        param_1[0x49] = -(dVar40 * dVar13) + dVar11 * dVar38;
        dVar14 = param_1[0x4a] + dVar15 * param_1[0x75];
        param_1[0x4a] = param_1[0x4b] + -(param_1[0x78] * dVar14) + dVar15 * param_1[0x76];
        param_1[0x4b] = -(param_1[0x79] * dVar14) + dVar15 * param_1[0x77];
        dVar7 = *param_1;
        dVar11 = param_1[0x73];
      }
      dVar6 = dVar5 * param_1[0x2f] + param_1[0x2e] * dVar9 + param_1[0x30] * dVar6;
      dVar5 = dVar24 * param_1[0x2f] + param_1[0x2e] * dVar23 + param_1[0x30] * dVar8;
      if (NAN(param_1[0x4c])) {
        dVar15 = 1.0;
        if (NAN(param_1[0x4d])) {
          dVar23 = 0.0;
          dVar8 = 0.0;
        }
        else {
          dVar15 = param_1[0x4d] + 1.0;
          dVar8 = param_1[0x4e];
          dVar23 = param_1[0x4f];
        }
        param_1[0x4d] = dVar15;
        param_1[0x4e] = dVar6 + dVar8;
        dVar8 = (dVar6 + dVar8) / dVar15;
        dVar5 = dVar5 + dVar23;
        param_1[0x4f] = dVar5;
        dVar6 = dVar5 / dVar15;
        if (dVar7 <= dVar11 * dVar15) {
          param_1[0x4c] = dVar8 * (1.0 - param_1[0x75]);
          param_1[0x4d] = dVar8 * (param_1[0x77] - param_1[0x79]);
          param_1[0x4e] = dVar6 * (1.0 - param_1[0x75]);
          dVar5 = dVar6 * (param_1[0x77] - param_1[0x79]);
          goto LAB_1027d329c;
        }
      }
      else {
        dVar8 = param_1[0x4c] + dVar6 * param_1[0x75];
        param_1[0x4c] = param_1[0x4d] + -(param_1[0x78] * dVar8) + dVar6 * param_1[0x76];
        param_1[0x4d] = -(param_1[0x79] * dVar8) + dVar6 * param_1[0x77];
        dVar6 = param_1[0x4e] + dVar5 * param_1[0x75];
        param_1[0x4e] = param_1[0x4f] + -(param_1[0x78] * dVar6) + dVar5 * param_1[0x76];
        dVar5 = -(param_1[0x79] * dVar6) + dVar5 * param_1[0x77];
LAB_1027d329c:
        param_1[0x4f] = dVar5;
      }
      dVar7 = param_1[5];
      if ((cVar4 == '\0') || (*(char *)((long)param_1 + 0x11) == '\0')) {
        if (*(char *)(param_1 + 2) == '\0') {
          func_0x000103463454(&local_90,0x103c130e0,0x18);
          dVar15 = param_1[0x31];
          dVar11 = param_1[0x7b];
          dVar8 = extraout_d16;
          dVar6 = extraout_d17;
          local_98 = dVar25;
          if (dVar15 < dVar11) goto LAB_1027d3374;
        }
        else {
          dVar5 = param_1[0x2e];
          dVar23 = param_1[0x2f];
          dVar19 = param_1[0x30];
          dVar15 = param_1[0x31];
          dVar8 = (((dVar8 - dVar27 / dVar11) - dVar5 * dVar25) - dVar23 * dVar35) - dVar19 * dVar43
          ;
          dVar6 = (((dVar32 / dVar11 + dVar6) - dVar5 * dVar20) - dVar23 * dVar21) - dVar19 * dVar22
          ;
          dStack_80 = param_1[0x7e];
          local_90 = param_1[0x7d];
          dVar5 = (-(dVar13 * dVar23) - dVar5 * dVar16) - dVar19 * dVar14;
          dVar11 = param_1[0x7b];
          local_88 = local_90;
          if (dVar15 < dVar11) {
LAB_1027d3374:
            dVar15 = dVar15 + param_1[0x7c];
            param_1[0x31] = dVar15;
          }
        }
      }
      else {
        dVar8 = param_1[0x53] - param_1[0x2e];
        dVar6 = param_1[0x54] - param_1[0x2f];
        dVar15 = param_1[0x31];
        dVar5 = param_1[0x55] - param_1[0x30];
        dVar14 = 1.0;
        dVar35 = 0.0;
        dVar43 = 0.0;
        local_90 = param_1[0x7f];
        dVar22 = 0.0;
        dVar16 = 0.0;
        dVar13 = 0.0;
        dVar21 = 1.0;
        dVar20 = 0.0;
        dVar25 = 1.0;
        dVar11 = param_1[0x7b];
        local_88 = local_90;
        dStack_80 = local_90;
        if (dVar15 < dVar11) goto LAB_1027d3374;
      }
      dVar23 = param_1[0x35];
      if (dVar23 < dVar11) {
        dVar23 = dVar23 + param_1[0x7c];
        param_1[0x35] = dVar23;
        dVar19 = param_1[0x39];
      }
      else {
        dVar19 = param_1[0x39];
      }
      if (dVar19 < dVar11) {
        dVar19 = dVar19 + param_1[0x7c];
        param_1[0x39] = dVar19;
      }
      if ((0.0 <= local_90) && (*(int *)(param_1 + 0x9f) == 1)) {
        dVar7 = dVar7 * 0.017453292519943295;
        dVar24 = -dVar7;
        dVar11 = dVar24;
        if ((dVar8 < dVar24) || (dVar11 = dVar7, dVar7 < dVar8)) {
          dVar8 = dVar11;
        }
        dVar11 = dVar24;
        if ((dVar6 < dVar24) || (dVar11 = dVar7, dVar7 < dVar6)) {
          dVar6 = dVar11;
        }
        dVar11 = dVar24;
        if ((dVar5 < dVar24) || (dVar11 = dVar7, dVar7 < dVar5)) {
          dVar5 = dVar11;
        }
        dVar27 = param_1[0x32];
        dVar38 = param_1[0x33];
        dVar11 = dVar27 * dVar35 + dVar25 * dVar15 + dVar43 * dVar38;
        dVar9 = dVar27 * dVar21 + dVar20 * dVar15 + dVar22 * dVar38;
        dVar32 = dVar27 * dVar13 + dVar16 * dVar15 + dVar14 * dVar38;
        dVar37 = param_1[0x34];
        dVar28 = param_1[0x36];
        dVar36 = param_1[0x37];
        dVar40 = dVar35 * dVar23 + dVar25 * dVar37 + dVar43 * dVar28;
        dVar12 = dVar21 * dVar23 + dVar20 * dVar37 + dVar22 * dVar28;
        dVar26 = dVar13 * dVar23 + dVar16 * dVar37 + dVar14 * dVar28;
        dVar33 = param_1[0x38];
        dVar29 = dVar35 * dVar33 + dVar25 * dVar36 + dVar43 * dVar19;
        dVar31 = dVar21 * dVar33 + dVar20 * dVar36 + dVar22 * dVar19;
        dVar34 = dVar13 * dVar33 + dVar16 * dVar36 + dVar14 * dVar19;
        dVar39 = dVar35 * dVar12 + dVar9 * dVar25 + dVar31 * dVar43;
        dVar41 = dVar35 * dVar26 + dVar32 * dVar25 + dVar34 * dVar43;
        dVar46 = dVar21 * dVar40 + dVar11 * dVar20 + dVar29 * dVar22;
        dVar48 = dVar21 * dVar26 + dVar32 * dVar20 + dVar34 * dVar22;
        dVar10 = dVar13 * dVar40 + dVar11 * dVar16 + dVar29 * dVar14;
        dVar30 = dVar13 * dVar12 + dVar9 * dVar16 + dVar31 * dVar14;
        dVar11 = local_90 + dVar35 * dVar40 + dVar11 * dVar25 + dVar29 * dVar43;
        dVar9 = local_88 + dVar21 * dVar12 + dVar9 * dVar20 + dVar31 * dVar22;
        dVar26 = dStack_80 + dVar13 * dVar26 + dVar32 * dVar16 + dVar34 * dVar14;
        dVar29 = -(dVar48 * dVar30) + dVar26 * dVar9;
        dVar32 = -(dVar46 * dVar26) + dVar10 * dVar48;
        dVar40 = -(dVar9 * dVar10) + dVar30 * dVar46;
        dVar12 = dVar39 * dVar32 + dVar29 * dVar11 + dVar40 * dVar41;
        dVar31 = 0.0;
        bVar1 = false;
        bVar2 = true;
        if (-2.220446049250313e-16 <= dVar12) {
          bVar1 = false;
          bVar2 = true;
          if (!NAN(dVar12)) {
            bVar1 = dVar12 == 2.220446049250313e-16;
            bVar2 = 2.220446049250313e-16 <= dVar12;
          }
        }
        dVar44 = 0.0;
        dVar42 = 0.0;
        dVar17 = 0.0;
        dVar47 = 0.0;
        dVar45 = 0.0;
        dVar34 = 0.0;
        dVar18 = 0.0;
        dVar49 = 0.0;
        if (bVar2 && !bVar1) {
          dVar31 = dVar29 / dVar12;
          dVar44 = (-(dVar39 * dVar26) + dVar30 * dVar41) / dVar12;
          dVar42 = (-(dVar41 * dVar9) + dVar48 * dVar39) / dVar12;
          dVar17 = dVar32 / dVar12;
          dVar47 = (-(dVar41 * dVar10) + dVar26 * dVar11) / dVar12;
          dVar45 = (-(dVar11 * dVar48) + dVar46 * dVar41) / dVar12;
          dVar34 = dVar40 / dVar12;
          dVar18 = (-(dVar11 * dVar30) + dVar10 * dVar39) / dVar12;
          dVar49 = (-(dVar39 * dVar46) + dVar9 * dVar11) / dVar12;
        }
        dVar11 = dVar20 * dVar17 + dVar31 * dVar25 + dVar34 * dVar16;
        dVar10 = dVar20 * dVar47 + dVar44 * dVar25 + dVar18 * dVar16;
        dVar30 = dVar21 * dVar17 + dVar31 * dVar35 + dVar34 * dVar13;
        dVar32 = dVar22 * dVar17 + dVar31 * dVar43 + dVar34 * dVar14;
        dVar26 = dVar20 * dVar45 + dVar42 * dVar25 + dVar49 * dVar16;
        dVar40 = dVar21 * dVar47 + dVar44 * dVar35 + dVar18 * dVar13;
        dVar12 = dVar22 * dVar47 + dVar44 * dVar43 + dVar18 * dVar14;
        dVar39 = dVar21 * dVar45 + dVar42 * dVar35 + dVar49 * dVar13;
        dVar34 = dVar22 * dVar45 + dVar42 * dVar43 + dVar49 * dVar14;
        dVar29 = dVar27 * dVar30 + dVar11 * dVar15 + dVar32 * dVar38;
        dVar9 = dVar27 * dVar40 + dVar10 * dVar15 + dVar12 * dVar38;
        dVar31 = dVar23 * dVar30 + dVar11 * dVar37 + dVar32 * dVar28;
        dVar11 = dVar33 * dVar30 + dVar11 * dVar36 + dVar32 * dVar19;
        dVar32 = dVar23 * dVar40 + dVar10 * dVar37 + dVar12 * dVar28;
        dVar40 = dVar33 * dVar40 + dVar10 * dVar36 + dVar12 * dVar19;
        dVar10 = dVar27 * dVar39 + dVar26 * dVar15 + dVar34 * dVar38;
        dVar41 = dVar23 * dVar39 + dVar26 * dVar37 + dVar34 * dVar28;
        dVar34 = dVar33 * dVar39 + dVar26 * dVar36 + dVar34 * dVar19;
        dVar39 = dVar20 * dVar9 + dVar25 * dVar29 + dVar16 * dVar10;
        dVar48 = dVar21 * dVar9 + dVar35 * dVar29 + dVar13 * dVar10;
        dVar12 = dVar22 * dVar9 + dVar43 * dVar29 + dVar14 * dVar10;
        dVar30 = dVar20 * dVar32 + dVar25 * dVar31 + dVar16 * dVar41;
        dVar26 = dVar21 * dVar32 + dVar35 * dVar31 + dVar13 * dVar41;
        dVar46 = dVar22 * dVar32 + dVar43 * dVar31 + dVar14 * dVar41;
        dVar16 = dVar20 * dVar40 + dVar25 * dVar11 + dVar16 * dVar34;
        dVar35 = dVar21 * dVar40 + dVar35 * dVar11 + dVar13 * dVar34;
        dVar13 = dVar22 * dVar40 + dVar43 * dVar11 + dVar14 * dVar34;
        dVar14 = param_1[0x2e] + dVar9 * dVar6 + dVar8 * dVar29 + dVar5 * dVar10;
        dVar9 = param_1[0x2f] + dVar6 * dVar32 + dVar8 * dVar31 + dVar5 * dVar41;
        dVar6 = param_1[0x30] + dVar6 * dVar40 + dVar8 * dVar11 + dVar5 * dVar34;
        param_1[0x31] = param_1[0x31] - (dVar37 * dVar48 + dVar15 * dVar39 + dVar36 * dVar12);
        param_1[0x32] = param_1[0x32] - (dVar23 * dVar48 + dVar27 * dVar39 + dVar33 * dVar12);
        param_1[0x33] = param_1[0x33] - (dVar28 * dVar48 + dVar38 * dVar39 + dVar19 * dVar12);
        param_1[0x34] = param_1[0x34] - (dVar37 * dVar26 + dVar15 * dVar30 + dVar36 * dVar46);
        param_1[0x35] = param_1[0x35] - (dVar23 * dVar26 + dVar27 * dVar30 + dVar33 * dVar46);
        param_1[0x36] = param_1[0x36] - (dVar28 * dVar26 + dVar38 * dVar30 + dVar19 * dVar46);
        param_1[0x37] = param_1[0x37] - (dVar37 * dVar35 + dVar15 * dVar16 + dVar36 * dVar13);
        param_1[0x38] = param_1[0x38] - (dVar23 * dVar35 + dVar27 * dVar16 + dVar33 * dVar13);
        param_1[0x39] = param_1[0x39] - (dVar28 * dVar35 + dVar38 * dVar16 + dVar19 * dVar13);
        param_1[0x2f] = dVar9;
        param_1[0x30] = dVar6;
        param_1[0x2e] = dVar14;
        dVar5 = dVar24;
        if ((dVar14 < dVar24) || (dVar5 = dVar7, dVar7 < dVar14)) {
          param_1[0x2e] = dVar5;
        }
        dVar5 = dVar24;
        if ((dVar9 < dVar24) || (dVar5 = dVar7, dVar7 < dVar9)) {
          param_1[0x2f] = dVar5;
        }
        local_98 = dVar25;
        if ((dVar6 < dVar24) || (dVar24 = dVar7, dVar7 < dVar6)) {
          param_1[0x30] = dVar24;
        }
      }
    }
LAB_1027d38ec:
    if (*_DAT_103fed798 == local_78) {
      return;
    }
    dVar5 = (double)func_0x0001034627dc();
    param_2 = extraout_x1;
  } while( true );
}

