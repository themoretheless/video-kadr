
/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(long param_1,long param_2,undefined8 param_3,undefined8 param_4,undefined8 param_5,
          undefined1 param_6,double *param_7,undefined8 param_8)

{
  int iVar1;
  undefined8 *puVar2;
  undefined1 auVar3 [16];
  undefined1 auVar4 [12];
  long local_150;
  long lStack_148;
  undefined8 local_140;
  undefined1 *puStack_138;
  double *local_130;
  undefined8 uStack_128;
  long *local_120;
  undefined8 uStack_118;
  double *local_110;
  undefined8 uStack_108;
  double local_100;
  double dStack_f8;
  long local_f0;
  undefined1 local_e1;
  undefined8 local_e0;
  undefined4 local_d8;
  undefined8 local_d0;
  undefined8 uStack_c8;
  undefined8 local_c0;
  undefined1 auStack_b8 [12];
  undefined2 local_ac;
  char local_a9;
  undefined8 local_98;
  undefined8 uStack_90;
  undefined8 local_78;
  long local_38;
  
  local_38 = *_DAT_103fed798;
  auVar3._0_8_ = (long)(int)*(undefined8 *)(param_1 + 0x60);
  auVar3._8_8_ = (long)(int)((ulong)*(undefined8 *)(param_1 + 0x60) >> 0x20);
  auVar3 = NEON_scvtf(auVar3,8);
  local_100 = *param_7 * auVar3._0_8_;
  dStack_f8 = param_7[1] * auVar3._8_8_;
  puStack_138 = &local_e1;
  local_120 = &local_f0;
  local_110 = &local_100;
  iVar1 = *(int *)(param_2 + 8);
  local_150 = param_2;
  lStack_148 = param_1;
  local_140 = param_5;
  local_130 = param_7;
  uStack_128 = param_4;
  uStack_118 = param_3;
  uStack_108 = param_8;
  local_f0 = param_1;
  local_e1 = param_6;
  if (0 < iVar1) {
    local_ac = 0x401;
    local_98 = 0;
    uStack_90 = 0;
    local_78 = 1;
    func_0x00010345e2e8(auStack_b8);
    local_c0 = 0;
    puVar2 = (undefined8 *)func_0x00010345e378(&local_c0,0x80);
    puVar2[7] = 0;
    puVar2[6] = 0;
    puVar2[5] = 0;
    puVar2[4] = 0;
    puVar2[3] = 0;
    puVar2[2] = 0;
    puVar2[1] = 0;
    *puVar2 = 0x1040b0db8;
    *(int *)(puVar2 + 8) = iVar1;
    *(undefined4 *)((long)puVar2 + 0x44) = 0;
    puVar2[9] = 1;
    puVar2[10] = &local_150;
    puVar2[0xb] = _DAT_1037beef0;
    iVar1 = func_0x00010345e30c(0);
    *(undefined4 *)(puVar2 + 0xe) = 0;
    *(undefined1 *)((long)puVar2 + 0x74) = 5;
    puVar2[0xf] = local_c0;
    local_e0 = 0;
    local_d8 = 1;
    uStack_c8 = 1;
    local_d0 = 1;
    puVar2[0xc] = &local_e0;
    puVar2[0xd] = ((long)iVar1 & 0x3fffffffffffffffU) << 1;
    func_0x00010345e324(puVar2,auStack_b8,&local_d0,auStack_b8);
    if (local_a9 != -1) {
      func_0x00010345e36c(auStack_b8);
    }
  }
  if (*_DAT_103fed798 == local_38) {
    return;
  }
  auVar4 = func_0x0001034627dc();
  if (auVar4._8_4_ == 0) {
    func_0x00010345b87c(auVar4._0_8_);
  }
  func_0x0001000164d0(auVar4._0_8_);
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}

