
void entry(undefined8 param_1,uint param_2)

{
  if (param_2 < 0xa5) {
                    /* WARNING: Could not emulate address calculation at 0x0001027b0f64 */
                    /* WARNING: Treating indirect jump as call */
    (*(code *)((ulong)*(ushort *)((ulong)param_2 * 2 + 0x103c120e0) * 4 + 0x1027b0f70))();
    return;
  }
  return;
}

