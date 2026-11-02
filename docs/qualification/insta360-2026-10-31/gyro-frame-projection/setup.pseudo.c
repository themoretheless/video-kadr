
/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Type propagation algorithm not settling */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(double param_1,long param_2,undefined8 *param_3,undefined4 param_4,int param_5)

{
  undefined8 *puVar1;
  ulong uVar2;
  double dVar3;
  long lVar4;
  ulong uVar5;
  undefined8 *******pppppppuVar6;
  long lVar7;
  double *pdVar8;
  undefined4 uVar9;
  double dVar10;
  undefined8 uVar11;
  double dVar12;
  undefined1 auVar13 [16];
  undefined1 auVar14 [16];
  double dVar15;
  undefined1 auVar16 [16];
  undefined7 local_180;
  undefined4 uStack_179;
  undefined1 local_175;
  char local_169;
  undefined8 *******local_168;
  ulong local_160;
  undefined8 uStack_158;
  undefined1 auStack_150 [272];
  
  puVar1 = (undefined8 *)(param_2 + 0x13c);
  lVar7 = param_3[1];
  if (lVar7 == 0xb) {
    pdVar8 = (double *)*param_3;
    dVar10 = pdVar8[1];
    dVar3 = *pdVar8;
    uVar11 = *(undefined8 *)(param_2 + 0x60);
    auVar14._0_8_ = (long)((int)uVar11 + -1);
    auVar14._8_8_ = (long)((int)((ulong)uVar11 >> 0x20) + -1);
    auVar14 = NEON_scvtf(auVar14,8);
    auVar16 = NEON_fmov(0xbff0000000000000,8);
    dVar12 = auVar14._0_8_ / (pdVar8[6] + auVar16._0_8_);
    dVar15 = auVar14._8_8_ / (pdVar8[7] + auVar16._8_8_);
    *(ulong *)(param_2 + 0xd8) = CONCAT44((float)(dVar15 * pdVar8[9]),(float)(dVar12 * pdVar8[8]));
    *(ulong *)(param_2 + 0xd0) = CONCAT44((float)(dVar15 * dVar10),(float)(dVar12 * dVar3));
    *(float *)(param_2 + 0x138) = (float)(param_1 * 0.5);
    dVar10 = pdVar8[3];
    dVar3 = pdVar8[2];
    *(ulong *)(param_2 + 0x144) = CONCAT44((float)pdVar8[5],(float)pdVar8[4]);
    *puVar1 = CONCAT44((float)dVar10,(float)dVar3);
    *(undefined4 *)(param_2 + 0x14c) = 0;
    uVar11 = NEON_scvtf(uVar11,4);
  }
  else {
    if (lVar7 == 0x14) {
      pdVar8 = (double *)*param_3;
      dVar10 = pdVar8[1];
      dVar3 = *pdVar8;
      uVar11 = *(undefined8 *)(param_2 + 0x60);
      auVar16._0_8_ = (long)((int)uVar11 + -1);
      auVar16._8_8_ = (long)((int)((ulong)uVar11 >> 0x20) + -1);
      auVar14 = NEON_scvtf(auVar16,8);
      auVar16 = NEON_fmov(0xbff0000000000000,8);
      dVar12 = auVar14._0_8_ / (pdVar8[0xf] + auVar16._0_8_);
      dVar15 = auVar14._8_8_ / (pdVar8[0x10] + auVar16._8_8_);
      *(ulong *)(param_2 + 0xd8) =
           CONCAT44((float)(dVar15 * pdVar8[0x12]),(float)(dVar12 * pdVar8[0x11]));
      *(ulong *)(param_2 + 0xd0) = CONCAT44((float)(dVar15 * dVar10),(float)(dVar12 * dVar3));
      *(float *)(param_2 + 0x128) = (float)pdVar8[0x13];
      *(float *)(param_2 + 0x138) = (float)(param_1 * 0.5);
      dVar10 = pdVar8[3];
      dVar3 = pdVar8[2];
      *(ulong *)(param_2 + 0x144) = CONCAT44((float)pdVar8[5],(float)pdVar8[4]);
      *puVar1 = CONCAT44((float)dVar10,(float)dVar3);
      dVar10 = pdVar8[7];
      dVar3 = pdVar8[6];
      *(ulong *)(param_2 + 0x154) = CONCAT44((float)pdVar8[9],(float)pdVar8[8]);
      *(ulong *)(param_2 + 0x14c) = CONCAT44((float)dVar10,(float)dVar3);
      dVar10 = pdVar8[0xb];
      dVar3 = pdVar8[10];
      *(ulong *)(param_2 + 0x164) = CONCAT44((float)pdVar8[0xd],(float)pdVar8[0xc]);
      *(ulong *)(param_2 + 0x15c) = CONCAT44((float)dVar10,(float)dVar3);
      *(float *)(param_2 + 0x16c) = (float)pdVar8[0xe];
    }
    else {
      if (lVar7 != 0xc) {
        lVar7 = 0x103d84e34;
        lVar4 = func_0x000103463bb0(0x103d84e34,0x2f);
        if (lVar4 != 0) {
          lVar7 = lVar4 + 1;
        }
        uVar5 = func_0x000103463b5c(lVar7);
        if (0x7ffffffffffffff7 < uVar5) {
          uVar11 = func_0x000100016eb8(&local_168);
          func_0x00010002bfd0(auStack_150);
          if (local_169 < '\0') goto LAB_101b55b64;
          while (-1 < (long)uStack_158) {
            func_0x00010345b87c(uVar11);
LAB_101b55b64:
            func_0x000103462518(CONCAT17((undefined1)uStack_179,local_180));
          }
          func_0x000103462518(local_168);
          func_0x00010345b87c(uVar11);
                    /* WARNING: Bad instruction - Truncating control flow here */
          halt_baddata();
        }
        if (uVar5 < 0x17) {
          uStack_158 = CONCAT17((char)uVar5,(undefined7)uStack_158);
          pppppppuVar6 = &local_168;
          if (uVar5 == 0) goto LAB_101b55a8c;
        }
        else {
          uVar2 = (uVar5 & 0xfffffffffffffff8) + 8;
          if ((uVar5 | 7) != 0x17) {
            uVar2 = uVar5 | 7;
          }
          pppppppuVar6 = (undefined8 *******)func_0x000103462620(uVar2 + 1);
          uStack_158 = uVar2 + 1 | 0x8000000000000000;
          local_168 = pppppppuVar6;
          local_160 = uVar5;
        }
        func_0x00010346343c(pppppppuVar6,lVar7,uVar5);
LAB_101b55a8c:
        *(undefined1 *)((long)pppppppuVar6 + uVar5) = 0;
        local_169 = '\v';
        local_180 = (undefined7)_DAT_103d84e81;
        uStack_179._0_1_ = (undefined1)((ulong)_DAT_103d84e81 >> 0x38);
        uStack_179 = CONCAT31(0x6d6172,(undefined1)uStack_179);
        local_175 = 0;
        func_0x00010002b8a0(auStack_150,&local_168,&local_180,0x150,2);
        func_0x000100016bb0(auStack_150,0x103d84e8d,0x23);
        func_0x00010002bfd0(auStack_150);
        if (local_169 < '\0') {
          func_0x000103462518(CONCAT17((undefined1)uStack_179,local_180));
        }
        if (-1 < (long)uStack_158) {
          return;
        }
        func_0x000103462518(local_168);
        return;
      }
      pdVar8 = (double *)*param_3;
      dVar10 = pdVar8[1];
      dVar3 = *pdVar8;
      uVar11 = *(undefined8 *)(param_2 + 0x60);
      auVar13._0_8_ = (long)((int)uVar11 + -1);
      auVar13._8_8_ = (long)((int)((ulong)uVar11 >> 0x20) + -1);
      auVar14 = NEON_scvtf(auVar13,8);
      auVar16 = NEON_fmov(0xbff0000000000000,8);
      dVar12 = auVar14._0_8_ / (pdVar8[7] + auVar16._0_8_);
      dVar15 = auVar14._8_8_ / (pdVar8[8] + auVar16._8_8_);
      *(ulong *)(param_2 + 0xd8) =
           CONCAT44((float)(dVar15 * pdVar8[10]),(float)(dVar12 * pdVar8[9]));
      *(ulong *)(param_2 + 0xd0) = CONCAT44((float)(dVar15 * dVar10),(float)(dVar12 * dVar3));
      *(float *)(param_2 + 0x128) = (float)pdVar8[0xb];
      *(float *)(param_2 + 0x138) = (float)(param_1 * 0.5);
      dVar10 = pdVar8[3];
      dVar3 = pdVar8[2];
      *(ulong *)(param_2 + 0x144) = CONCAT44((float)pdVar8[5],(float)pdVar8[4]);
      *puVar1 = CONCAT44((float)dVar10,(float)dVar3);
      *(float *)(param_2 + 0x14c) = (float)pdVar8[6];
    }
    uVar11 = NEON_scvtf(uVar11,4);
  }
  *(undefined8 *)(param_2 + 0xe0) = uVar11;
  *(undefined4 *)(param_2 + 300) = param_4;
  uVar9 = 0x3f800000;
  if (param_5 != 0) {
    uVar9 = 0xbf800000;
  }
  *(undefined4 *)(param_2 + 0x134) = uVar9;
  return;
}

