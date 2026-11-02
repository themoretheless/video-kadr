/* REA/Ghidra pseudocode observation; not original source or product implementation. */

undefined8
__ZN10extra_info16OffsetSerializer18ParseBase64EncodedERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEERNS_12CameraOffsetE
          (undefined8 param_1,undefined8 param_2,undefined8 param_3)

{
  undefined8 uVar1;
  undefined8 local_38 [2];
  char local_21;
  
  uVar1 = __Z13base64_decodeRKNSt3__112basic_stringIcNS_11char_traitsIcEENS_9allocatorIcEEEE
                    (local_38,param_2);
  uVar1 = __ZN10extra_info16OffsetSerializer5ParseERKNSt3__112basic_stringIcNS1_11char_traitsIcEENS1_9allocatorIcEEEERNS_12CameraOffsetE
                    (uVar1,local_38,param_3);
  if (-1 < local_21) {
    return uVar1;
  }
  __ZdlPv(local_38[0]);
  return uVar1;
}

