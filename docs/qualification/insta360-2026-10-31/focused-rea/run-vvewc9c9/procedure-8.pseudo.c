/* REA/Ghidra pseudocode observation; not original source or product implementation. */

undefined8
__ZN10extra_info16OffsetSerializer5ParseERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEERNS_12CameraOffsetE
          (undefined8 param_1,long param_2,long *param_3)

{
  char *pcVar1;
  ulong uVar2;
  bool bVar3;
  undefined8 uVar4;
  long *plVar5;
  char *pcVar6;
  ulong uVar7;
  long lVar8;
  long lVar9;
  undefined8 local_2b0 [2];
  char local_299;
  undefined8 local_298 [2];
  char local_281;
  undefined1 auStack_280 [276];
  int local_16c;
  uint local_168;
  char local_161;
  undefined **local_160 [2];
  undefined **local_150 [8];
  undefined8 local_110;
  char local_f9;
  undefined **appuStack_e8 [19];
  
  FUN_1002a8b98(local_160,param_2,8);
  local_168 = 0;
  uVar4 = __ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERj(local_160,&local_168);
  plVar5 = (long *)FUN_1002a8cd4(uVar4,&local_161);
  if ((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) == 0) {
    uVar4 = 0;
    if ((local_161 != '_') || (uVar7 = (ulong)local_168, local_168 == 0)) goto LAB_101b5f8d8;
    uVar2 = *(ulong *)(param_2 + 8);
    if (-1 < (char)*(byte *)(param_2 + 0x17)) {
      uVar2 = (ulong)*(byte *)(param_2 + 0x17);
    }
    if (uVar2 < local_168 * 0xc) goto LAB_101b5f8d4;
    lVar8 = param_3[1] - *param_3 >> 3;
    bVar3 = uVar7 < (ulong)(lVar8 * -0x5555555555555555);
    lVar8 = uVar7 + lVar8 * 0x5555555555555555;
    if (bVar3 || lVar8 == 0) {
      if (bVar3) {
        param_3[1] = *param_3 + (ulong)local_168 * 0x18;
      }
LAB_101b5f9d8:
      lVar8 = 0;
      uVar7 = 0;
      do {
        lVar9 = *param_3;
        uVar4 = __ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERf(local_160,lVar9 + lVar8);
        plVar5 = (long *)FUN_1002a8cd4(uVar4,&local_161);
        uVar4 = 0;
        if (((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) != 0) ||
           (local_161 != '_')) goto LAB_101b5f8d8;
        uVar4 = __ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERf(local_160,lVar9 + lVar8 + 4);
        plVar5 = (long *)FUN_1002a8cd4(uVar4,&local_161);
        uVar4 = 0;
        if (((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) != 0) ||
           (local_161 != '_')) goto LAB_101b5f8d8;
        uVar4 = __ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERf(local_160,lVar9 + lVar8 + 8);
        plVar5 = (long *)FUN_1002a8cd4(uVar4,&local_161);
        uVar4 = 0;
        if (((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) != 0) ||
           (local_161 != '_')) goto LAB_101b5f8d8;
        uVar4 = __ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERf(local_160,lVar9 + lVar8 + 0xc)
        ;
        plVar5 = (long *)FUN_1002a8cd4(uVar4,&local_161);
        uVar4 = 0;
        if (((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) != 0) ||
           (local_161 != '_')) goto LAB_101b5f8d8;
        uVar4 = __ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERf
                          (local_160,lVar9 + lVar8 + 0x10);
        plVar5 = (long *)FUN_1002a8cd4(uVar4,&local_161);
        uVar4 = 0;
        if (((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) != 0) ||
           (local_161 != '_')) goto LAB_101b5f8d8;
        uVar4 = __ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERf
                          (local_160,lVar9 + lVar8 + 0x14);
        plVar5 = (long *)FUN_1002a8cd4(uVar4,&local_161);
        uVar4 = 0;
        if (((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) != 0) ||
           (local_161 != '_')) goto LAB_101b5f8d8;
        uVar7 = uVar7 + 1;
        lVar8 = lVar8 + 0x18;
      } while (uVar7 < local_168);
    }
    else {
      FUN_10163a674(param_3,lVar8);
      if (local_168 != 0) goto LAB_101b5f9d8;
    }
    uVar4 = __ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERj(local_160,(long)param_3 + 0x1c);
    plVar5 = (long *)FUN_1002a8cd4(uVar4,&local_161);
    if (((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) == 0) &&
       (((local_161 == '_' || (local_161 == '.')) &&
        (plVar5 = (long *)__ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERj
                                    (local_160,param_3 + 4),
        (*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) == 0)))) {
      plVar5 = (long *)FUN_1002a8cd4(local_160,&local_161);
      uVar4 = 1;
      if (((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) != 0) ||
         (local_161 != '_')) goto LAB_101b5f8d8;
      local_16c = -1;
      plVar5 = (long *)__ZNSt3__113basic_istreamIcNS_11char_traitsIcEEErsERi(local_160,&local_16c);
      if ((*(byte *)((long)plVar5 + *(long *)(*plVar5 + -0x18) + 0x20) & 5) == 0) {
        if (0 < local_16c) {
          *(int *)(param_3 + 3) = local_16c;
          goto LAB_101b5f8d8;
        }
        pcVar1 = 
        "/Users/insta360/jenkins/workspace/Studio_Mac/build/studio_src/deps/bmg/ExtraInfoTools/src/extra_info/offset_serializer.cpp"
        ;
        pcVar6 = _strrchr("/Users/insta360/jenkins/workspace/Studio_Mac/build/studio_src/deps/bmg/ExtraInfoTools/src/extra_info/offset_serializer.cpp"
                          ,0x2f);
        if (pcVar6 != (char *)0x0) {
          pcVar1 = pcVar6 + 1;
        }
        FUN_101e51504(local_298,pcVar1);
        FUN_101e51504(local_2b0,"Parse");
        __ZN3ins11LLogMessageC1ERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEES9_ii
                  (auStack_280,local_298,local_2b0,0x47,3);
        FUN_100016bb0(auStack_280,
                      "offset pasre error:lens types is little than 0,maybe parse V3 offset in V1 format !!\n"
                      ,0x55);
        __ZN3ins11LLogMessageD1Ev(auStack_280);
        if (local_299 < '\0') {
          __ZdlPv(local_2b0[0]);
        }
        if (local_281 < '\0') {
          __ZdlPv(local_298[0]);
        }
      }
    }
  }
LAB_101b5f8d4:
  uVar4 = 0;
LAB_101b5f8d8:
  local_160[0] = &PTR_FUN_1040efce0;
  appuStack_e8[0] = &PTR_FUN_1040efd08;
  local_150[0] = &PTR_FUN_1040720e8;
  if (local_f9 < '\0') {
    __ZdlPv(local_110);
  }
  __ZNSt3__115basic_streambufIcNS_11char_traitsIcEEED2Ev(local_150);
  __ZNSt3__113basic_istreamIcNS_11char_traitsIcEEED2Ev(local_160,&PTR_PTR_1040efd20);
  __ZNSt3__19basic_iosIcNS_11char_traitsIcEEED2Ev(appuStack_e8);
  return uVar4;
}

