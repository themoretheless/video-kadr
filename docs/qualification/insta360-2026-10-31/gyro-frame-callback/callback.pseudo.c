
void entry(long param_1,undefined8 *param_2,undefined8 *param_3,undefined8 *param_4,
          undefined4 *param_5,uint param_6)

{
  long lVar1;
  long *plVar2;
  undefined1 auVar3 [16];
  undefined1 auVar4 [16];
  undefined1 auVar5 [16];
  undefined8 local_c8;
  long *local_c0;
  undefined8 local_b8;
  undefined8 uStack_b0;
  undefined8 local_a8;
  undefined8 uStack_a0;
  undefined8 local_98;
  undefined8 uStack_90;
  undefined8 uStack_88;
  undefined8 local_80;
  long *local_78;
  undefined8 local_70;
  undefined8 uStack_68;
  undefined8 uStack_60;
  undefined8 uStack_58;
  undefined8 local_50;
  undefined8 uStack_48;
  undefined8 local_40;
  
  if (param_6 == 0) {
    auVar4 = NEON_fmov(0x3ff0000000000000,8);
    *(undefined8 *)(param_1 + 0x88) = 0;
    local_c8 = *param_2;
    local_c0 = (long *)param_2[1];
    if (local_c0 == (long *)0x0) goto LAB_101b556c8;
LAB_101b55678:
    plVar2 = local_c0 + 1;
    *plVar2 = *plVar2 + 1;
    local_b8 = *param_3;
    uStack_b0 = param_3[1];
    uStack_a0 = param_3[3];
    local_a8 = param_3[2];
    local_98 = param_3[4];
    uStack_88 = auVar4._8_8_;
    uStack_90 = auVar4._0_8_;
    *plVar2 = *plVar2 + 1;
  }
  else {
    auVar3._0_8_ = (long)((int)*param_4 + -1);
    auVar3._8_8_ = (long)((int)((ulong)*param_4 >> 0x20) + -1);
    auVar3 = NEON_scvtf(auVar3,8);
    auVar5 = NEON_fmov(0x3ff0000000000000,8);
    auVar4._0_8_ = auVar5._0_8_ / auVar3._0_8_;
    auVar4._8_8_ = auVar5._8_8_ / auVar3._8_8_;
    *(double *)(param_1 + 0x88) = (double)param_6;
    local_c8 = *param_2;
    local_c0 = (long *)param_2[1];
    if (local_c0 != (long *)0x0) goto LAB_101b55678;
LAB_101b556c8:
    local_b8 = *param_3;
    uStack_b0 = param_3[1];
    uStack_a0 = param_3[3];
    local_a8 = param_3[2];
    local_98 = param_3[4];
    uStack_88 = auVar4._8_8_;
    uStack_90 = auVar4._0_8_;
  }
  uStack_68 = uStack_b0;
  local_70 = local_b8;
  uStack_58 = uStack_a0;
  uStack_60 = local_a8;
  uStack_48 = uStack_90;
  local_50 = local_98;
  local_40 = uStack_88;
  local_80 = local_c8;
  local_78 = local_c0;
  func_0x000101b598ac(param_1 + 0x20,&local_80);
  plVar2 = local_78;
  if (local_78 != (long *)0x0) {
    LOAcquire();
    lVar1 = local_78[1];
    local_78[1] = lVar1 + -1;
    LORelease();
    if (lVar1 == 0) {
      (**(code **)(*local_78 + 0x10))(local_78);
      func_0x000103462194(plVar2);
    }
  }
  if (local_c0 == (long *)0x0) {
LAB_101b55758:
    plVar2 = *(long **)(param_1 + 0x58);
    *(undefined8 *)(param_1 + 0x58) = 0;
    if (plVar2 != (long *)(param_1 + 0x40)) goto LAB_101b5576c;
LAB_101b557a4:
    plVar2 = (long *)(param_1 + 0x40);
    lVar1 = 4;
  }
  else {
    LOAcquire();
    lVar1 = local_c0[1];
    local_c0[1] = lVar1 + -1;
    LORelease();
    if (lVar1 != 0) goto LAB_101b55758;
    (**(code **)(*local_c0 + 0x10))(local_c0);
    func_0x000103462194(local_c0);
    plVar2 = *(long **)(param_1 + 0x58);
    *(undefined8 *)(param_1 + 0x58) = 0;
    if (plVar2 == (long *)(param_1 + 0x40)) goto LAB_101b557a4;
LAB_101b5576c:
    if (plVar2 == (long *)0x0) goto LAB_101b557b8;
    lVar1 = 5;
  }
  (**(code **)(*plVar2 + lVar1 * 8))();
LAB_101b557b8:
  *(undefined4 *)(param_1 + 0x60) = *(undefined4 *)param_4;
  *(undefined4 *)(param_1 + 100) = *(undefined4 *)((long)param_4 + 4);
  *(undefined4 *)(param_1 + 0x68) = *param_5;
  *(undefined4 *)(param_1 + 0x6c) = param_5[1];
  return;
}

