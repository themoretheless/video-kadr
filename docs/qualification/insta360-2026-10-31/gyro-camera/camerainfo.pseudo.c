
/* WARNING: Globals starting with '_' overlap smaller symbols at the same address */

undefined8 entry(undefined8 *param_1,int param_2,int param_3)

{
  long lVar1;
  undefined8 uVar2;
  long lVar3;
  undefined8 local_180 [2];
  char local_169;
  undefined8 local_168 [2];
  char local_151;
  undefined1 auStack_150 [272];
  
  if (param_2 - 2U < 0xf1) {
                    /* WARNING: Could not emulate address calculation at 0x0001018b4310 */
                    /* WARNING: Treating indirect jump as call */
    uVar2 = (*(code *)((ulong)*(ushort *)((ulong)(param_2 - 2U) * 2 + 0x103b1d47e) * 4 + 0x1018b431c
                      ))();
    return uVar2;
  }
  if ((param_2 == 0xd) && (param_3 == 4)) {
    lVar1 = 0x103d6b6ae;
    lVar3 = func_0x000103463bb0(0x103d6b6ae,0x2f);
    if (lVar3 != 0) {
      lVar1 = lVar3 + 1;
    }
    func_0x000101e51504(local_168,lVar1);
    func_0x000101e51504(local_180,0x103d6b9cd);
    func_0x00010002b8a0(auStack_150,local_168,local_180,0x1d8,2);
    func_0x000100016bb0(auStack_150,0x103d6badc,0x4d);
    func_0x00010002bfd0(auStack_150);
    if (local_169 < '\0') {
      func_0x000103462518(local_180[0]);
    }
    if (local_151 < '\0') {
      func_0x000103462518(local_168[0]);
    }
    *param_1 = 0x4034000000000000;
    *(undefined1 *)(param_1 + 1) = 1;
  }
  else {
    if ((param_2 != 0xd) || (param_3 != 0xb)) {
      lVar1 = 0x103d6b6ae;
      lVar3 = func_0x000103463bb0(0x103d6b6ae,0x2f);
      if (lVar3 != 0) {
        lVar1 = lVar3 + 1;
      }
      func_0x000101e51504(local_168,lVar1);
      func_0x000101e51504(local_180,0x103d6b9cd);
      func_0x00010002b8a0(auStack_150,local_168,local_180,0x1e7,3);
      func_0x000100016bb0(auStack_150,0x103d6bb2a,0x11);
      func_0x00010002bfd0(auStack_150);
      if (local_169 < '\0') {
        func_0x000103462518(local_180[0]);
      }
      if (local_151 < '\0') {
        func_0x000103462518(local_168[0]);
      }
      return 0;
    }
    lVar1 = 0x103d6b6ae;
    lVar3 = func_0x000103463bb0(0x103d6b6ae,0x2f);
    if (lVar3 != 0) {
      lVar1 = lVar3 + 1;
    }
    func_0x000101e51504(local_168,lVar1);
    func_0x000101e51504(local_180,0x103d6b9cd);
    func_0x00010002b8a0(auStack_150,local_168,local_180,0x1e0,2);
    func_0x000100016bb0(auStack_150,0x103d6badc,0x4d);
    func_0x00010002bfd0(auStack_150);
    if (local_169 < '\0') {
      func_0x000103462518(local_180[0]);
    }
    if (local_151 < '\0') {
      func_0x000103462518(local_168[0]);
    }
    *param_1 = 0x4034000000000000;
  }
  param_1[3] = _DAT_1040a09f8;
  return 0;
}

