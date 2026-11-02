
/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(undefined8 param_1,undefined8 param_2,long *param_3,undefined8 param_4,undefined8 param_5
          )

{
  undefined8 uVar1;
  int iVar2;
  undefined8 uVar3;
  undefined8 *puVar4;
  undefined8 uVar5;
  long lVar6;
  ulong uVar7;
  undefined1 auVar8 [12];
  undefined8 *local_188;
  ulong *local_180;
  long lStack_178;
  undefined8 local_170;
  ulong *puStack_168;
  undefined8 *local_160;
  undefined8 *local_158;
  undefined8 uStack_150;
  undefined8 local_148;
  undefined8 local_140;
  ulong uStack_138;
  ulong local_130;
  undefined8 local_128;
  undefined8 uStack_120;
  undefined8 local_118;
  undefined4 local_110;
  undefined8 local_108;
  undefined8 uStack_100;
  undefined8 local_f8;
  undefined1 auStack_f0 [12];
  undefined2 local_e4;
  char local_e1;
  undefined8 local_d0;
  undefined8 uStack_c8;
  undefined8 local_b0;
  long local_70;
  
  uVar1 = uRam00000001037bd7b8;
  uVar5 = _DAT_1037bd7b0;
  local_70 = *_DAT_103fed798;
  local_130 = 1;
  lVar6 = *param_3;
  local_128 = param_2;
  uStack_120 = param_1;
  if (0x10 < (ulong)(param_3[1] - lVar6)) {
    do {
      uVar3 = func_0x0001027b724c(*(undefined8 *)(lVar6 + local_130 * 0x10));
      lStack_178 = func_0x0001027b7258(*(undefined8 *)(*param_3 + local_130 * 0x10));
      uVar7 = *(ulong *)(lStack_178 + 0x18);
      local_140 = 0x20;
      local_188 = &local_140;
      local_180 = &uStack_138;
      puStack_168 = &local_130;
      local_160 = &uStack_120;
      local_158 = &local_128;
      local_e4 = 0x401;
      local_d0 = 0;
      uStack_c8 = 0;
      local_b0 = 1;
      local_170 = param_5;
      uStack_150 = param_4;
      local_148 = uVar3;
      uStack_138 = uVar7;
      func_0x00010345e2e8(auStack_f0);
      local_f8 = 0;
      puVar4 = (undefined8 *)func_0x00010345e378(&local_f8,0xc0);
      puVar4[6] = 0;
      puVar4[5] = 0;
      puVar4[4] = 0;
      puVar4[3] = 0;
      puVar4[2] = 0;
      puVar4[1] = 0;
      *puVar4 = 0x1040b1310;
      puVar4[7] = 0;
      puVar4[8] = (uVar7 >> 5) + 1;
      puVar4[10] = uVar1;
      puVar4[9] = uVar5;
      puVar4[0xb] = &local_188;
      puVar4[0xd] = uVar1;
      puVar4[0xc] = uVar5;
      iVar2 = func_0x00010345e30c(0);
      *(undefined4 *)(puVar4 + 0x10) = 0;
      *(undefined1 *)((long)puVar4 + 0x84) = 5;
      puVar4[0x11] = local_f8;
      local_118 = 0;
      local_110 = 1;
      uStack_100 = 1;
      local_108 = 1;
      puVar4[0xe] = &local_118;
      puVar4[0xf] = ((long)iVar2 & 0x3fffffffffffffffU) << 1;
      func_0x00010345e324(puVar4,auStack_f0,&local_108,auStack_f0);
      if (local_e1 != -1) {
        func_0x00010345e36c(auStack_f0);
      }
      local_130 = local_130 + 1;
      lVar6 = *param_3;
    } while (local_130 < (ulong)(param_3[1] - lVar6 >> 4));
  }
  if (*_DAT_103fed798 == local_70) {
    return;
  }
  auVar8 = func_0x0001034627dc();
  uVar5 = auVar8._0_8_;
  if (auVar8._8_4_ != 0) {
    uVar5 = func_0x0001000164d0(uVar5);
  }
  func_0x00010345b87c(uVar5);
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}

