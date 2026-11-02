/* REA/Ghidra pseudocode observation; not original source or product implementation. */

void __ZN3pro22DeepTrackRenderManager24CreateDeepTrackerOptionsERKNSt3__110shared_ptrINS_22DeepTrackConfigurationEEEbf
               (undefined2 *param_1,undefined4 param_2,undefined1 param_3 [16],double param_4,
               double param_5,undefined8 param_6,long *param_7,int param_8)

{
  undefined1 uVar1;
  undefined4 uVar2;
  undefined8 uVar3;
  long lVar4;
  undefined8 *puVar5;
  double dVar6;
  double dVar7;
  undefined8 local_68;
  undefined8 uStack_60;
  undefined8 local_58;
  
  lVar4 = *param_7;
  *param_1 = 0;
  *(undefined8 *)(param_1 + 2) = 0;
  puVar5 = (undefined8 *)(param_1 + 0x1c);
  *(undefined8 *)(param_1 + 0x20) = 0;
  *puVar5 = 0;
  *(undefined8 *)(param_1 + 10) = 0;
  *(undefined8 *)(param_1 + 6) = 0;
  *(undefined4 *)(param_1 + 0xe) = 0;
  dVar7 = 2.075195144470854e-05;
  *(undefined8 *)(param_1 + 0x14) = 0x3dcccccd3eeb851f;
  *(undefined8 *)(param_1 + 0x10) = 0x3ef5c28f3e99999a;
  *(undefined4 *)(param_1 + 0x18) = 0x3e800000;
  *(undefined8 *)(param_1 + 0x28) = 0;
  *(undefined8 *)(param_1 + 0x24) = 0;
  *(undefined8 *)(param_1 + 0x30) = 0;
  *(undefined8 *)(param_1 + 0x2c) = 0;
  *(undefined8 *)(param_1 + 0x38) = 0;
  *(undefined8 *)(param_1 + 0x34) = 0;
  *(undefined8 *)(param_1 + 0x40) = 0;
  *(undefined8 *)(param_1 + 0x3c) = 0;
  *(undefined8 *)(param_1 + 0x48) = 0;
  *(undefined8 *)(param_1 + 0x44) = 0;
  *(undefined8 *)(param_1 + 0x50) = 0;
  *(undefined8 *)(param_1 + 0x4c) = 0;
  *(undefined8 *)(param_1 + 0x58) = 0;
  *(undefined8 *)(param_1 + 0x54) = 0;
  *(undefined8 *)(param_1 + 0x60) = 0;
  *(undefined8 *)(param_1 + 0x5c) = 0;
  *(undefined8 *)(param_1 + 0x68) = 0;
  *(undefined8 *)(param_1 + 100) = 0;
  *(undefined8 *)(param_1 + 0x6c) = 0;
  if (lVar4 != 0) {
    __ZN3pro22DeepTrackConfiguration23GetBigboomBallModelFileEv(&local_68);
    if (*(char *)((long)param_1 + 0x7f) < '\0') {
      __ZdlPv(*(undefined8 *)(param_1 + 0x34));
    }
    *(undefined8 *)(param_1 + 0x38) = uStack_60;
    *(undefined8 *)(param_1 + 0x34) = local_68;
    *(undefined8 *)(param_1 + 0x3c) = local_58;
    __ZN3pro22DeepTrackConfiguration19GetBigboomModelFileEv(&local_68);
    if (*(char *)((long)param_1 + 0x67) < '\0') {
      __ZdlPv(*(undefined8 *)(param_1 + 0x28));
    }
    *(undefined8 *)(param_1 + 0x2c) = uStack_60;
    *(undefined8 *)(param_1 + 0x28) = local_68;
    *(undefined8 *)(param_1 + 0x30) = local_58;
    __ZN3pro22DeepTrackConfiguration24GetDeepTrackModelCfgPathEv(&local_68);
    if (*(char *)((long)param_1 + 0xaf) < '\0') {
      __ZdlPv(*(undefined8 *)(param_1 + 0x4c));
    }
    *(undefined8 *)(param_1 + 0x50) = uStack_60;
    *(undefined8 *)(param_1 + 0x4c) = local_68;
    *(undefined8 *)(param_1 + 0x54) = local_58;
    __ZN3pro22DeepTrackConfiguration21GetDeepTrackModelPathEv(&local_68);
    if (*(char *)((long)param_1 + 199) < '\0') {
      __ZdlPv(*(undefined8 *)(param_1 + 0x58));
    }
    *(undefined8 *)(param_1 + 0x5c) = uStack_60;
    *(undefined8 *)(param_1 + 0x58) = local_68;
    *(undefined8 *)(param_1 + 0x60) = local_58;
    __ZN3pro22DeepTrackConfiguration19GetBigboomNamesFileEv(&local_68);
    if (*(char *)((long)param_1 + 0x4f) < '\0') {
      __ZdlPv(*puVar5);
    }
    *(undefined8 *)(param_1 + 0x20) = uStack_60;
    *puVar5 = local_68;
    *(undefined8 *)(param_1 + 0x24) = local_58;
    uVar1 = __ZNK3pro22DeepTrackConfiguration14IsCenterLockedEv(*param_7);
    *(undefined1 *)param_1 = uVar1;
    dVar6 = (double)__ZNK3pro22DeepTrackConfiguration6GetRoiEv(*param_7);
    *(float *)(param_1 + 0x12) = (float)dVar6;
    *(float *)(param_1 + 0x14) = (float)dVar7;
    *(float *)(param_1 + 0x16) = (float)param_4;
    *(float *)(param_1 + 0x18) = (float)param_5;
    uVar2 = __ZNK3pro22DeepTrackConfiguration14GetRenderWidthEv(*param_7);
    *(undefined4 *)(param_1 + 2) = uVar2;
    uVar2 = __ZNK3pro22DeepTrackConfiguration15GetRenderHeightEv(*param_7);
    *(undefined4 *)(param_1 + 4) = uVar2;
    uVar3 = __ZNK3pro22DeepTrackConfiguration11GetKindNameEv(*param_7);
    __ZNSt3__112basic_stringIcNS_11char_traitsIcEENS_9allocatorIcEEEaSERKS5_(param_1 + 100,uVar3);
    uVar1 = __ZNK3pro22DeepTrackConfiguration8IsManualEv(*param_7);
    *(undefined1 *)((long)param_1 + 1) = uVar1;
    uVar2 = __ZN6studio11DeepTracker20GetDefaultTrackWidthEv();
    *(undefined4 *)(param_1 + 6) = uVar2;
    uVar2 = __ZN6studio11DeepTracker21GetDefaultTrackHeightEv();
    *(undefined4 *)(param_1 + 8) = uVar2;
    *(undefined8 *)(param_1 + 10) = 0x40278d363eb2b8c2;
    if (param_8 != 0) {
      *(undefined4 *)(param_1 + 0xe) = param_2;
    }
  }
  return;
}

