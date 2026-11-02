
/* WARNING: Control flow encountered bad instruction data */
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

void entry(long param_1,uint *param_2,undefined8 param_3)

{
  int extraout_w1;
  undefined4 local_190 [2];
  undefined8 **local_188;
  undefined8 uStack_180;
  uint local_178 [2];
  uint *local_170;
  undefined8 uStack_168;
  undefined8 *local_160;
  uint *local_158;
  undefined8 uStack_150;
  undefined8 local_100;
  uint *puStack_f8;
  undefined8 uStack_f0;
  undefined2 local_e4;
  undefined8 local_d0;
  undefined8 uStack_c8;
  undefined8 local_b0;
  long local_68;
  
  local_68 = *_DAT_103fed798;
  if ((*param_2 & 0xff8) == 0x10) {
    func_0x00010345d910(&local_100);
    func_0x00010345d910(&local_160);
    uStack_168 = 0;
    local_178[0] = 0x1010000;
    local_190[0] = 0x2010000;
    uStack_180 = 0;
    local_188 = (undefined8 **)&local_100;
    local_170 = param_2;
    func_0x00010345dd6c(local_178,local_190,6,0);
    uStack_168 = 0;
    local_178[0] = 0x1010000;
    local_190[0] = 0x2010000;
    uStack_180 = 0;
    local_188 = &local_160;
    local_170 = (uint *)param_3;
    func_0x00010345dd6c(local_178,local_190,6,0);
    func_0x00010345d988(param_2,&local_100);
    func_0x00010345d988(param_3,&local_160);
    func_0x00010345d958(&local_160);
    func_0x00010345d958(&local_100);
  }
  uStack_f0 = 0;
  local_100._0_4_ = 0x1010000;
  local_160._0_4_ = 0x2010000;
  uStack_150 = 0;
  local_158 = param_2;
  puStack_f8 = param_2;
  func_0x00010345d664((double)*(float *)(param_1 + 0x30c),0,&local_100,&local_160,0x500000005,4);
  uStack_f0 = 0;
  local_100 = CONCAT44(local_100._4_4_,0x1010000);
  local_160 = (undefined8 *)CONCAT44(local_160._4_4_,0x2010000);
  uStack_150 = 0;
  local_158 = (uint *)param_3;
  puStack_f8 = (uint *)param_3;
  func_0x00010345d664((double)*(float *)(param_1 + 0x30c),0,&local_100,&local_160,0x500000005,4);
  puStack_f8 = (uint *)uRam00000001037bd518;
  local_100 = _DAT_1037bd510;
  local_e4 = 0x601;
  local_d0 = 0;
  uStack_c8 = 0;
  local_b0 = 8;
  func_0x00010345e2e8(&uStack_f0);
  if (*(uint *)(param_1 + 4) < 5) {
                    /* WARNING: Could not emulate address calculation at 0x00010274de4c */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)((ulong)*(byte *)((ulong)*(uint *)(param_1 + 4) + 0x103c0bf0d) * 4 + 0x10274de58))();
    return;
  }
  local_178[0] = local_178[0] & 0xffffff00;
  local_160 = &local_100;
  func_0x00010129c2e0(&local_160,local_178,&local_100);
  func_0x00010129c440(&local_100);
  if (*_DAT_103fed798 == local_68) {
    return;
  }
  func_0x0001034627dc();
  if (extraout_w1 == 0) {
    func_0x00010345b87c();
  }
  func_0x0001000164d0();
                    /* WARNING: Bad instruction - Truncating control flow here */
  halt_baddata();
}

