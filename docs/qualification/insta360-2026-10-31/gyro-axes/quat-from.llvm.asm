__ZN9arvrender14StabilizerUtil19QuatFromGyrostablibERKN3glm5tquatIfLNS1_9precisionE0EEE:
1018b928c:	48 8d 01 d0	adrp	x8, 12714 ; 0x104a63000
1018b9290:	08 c1 36 91	add	x8, x8, #0xdb0
1018b9294:	00 05 42 2d	ldp	s0, s1, [x8, #0x10]
1018b9298:	02 0d 43 2d	ldp	s2, s3, [x8, #0x18]
1018b929c:	04 14 40 2d	ldp	s4, s5, [x0]
1018b92a0:	06 88 24 1e	fnmul	s6, s0, s4
1018b92a4:	10 1c 41 2d	ldp	s16, s7, [x0, #0x8]
1018b92a8:	66 18 07 1f	fmadd	s6, s3, s7, s6
1018b92ac:	26 98 05 1f	fmsub	s6, s1, s5, s6
1018b92b0:	46 98 10 1f	fmsub	s6, s2, s16, s6
1018b92b4:	11 08 27 1e	fmul	s17, s0, s7
1018b92b8:	71 44 04 1f	fmadd	s17, s3, s4, s17
1018b92bc:	31 44 10 1f	fmadd	s17, s1, s16, s17
1018b92c0:	51 c4 05 1f	fmsub	s17, s2, s5, s17
1018b92c4:	32 08 27 1e	fmul	s18, s1, s7
1018b92c8:	72 48 05 1f	fmadd	s18, s3, s5, s18
1018b92cc:	52 48 04 1f	fmadd	s18, s2, s4, s18
1018b92d0:	12 c8 10 1f	fmsub	s18, s0, s16, s18
1018b92d4:	42 08 27 1e	fmul	s2, s2, s7
1018b92d8:	62 08 10 1f	fmadd	s2, s3, s16, s2
1018b92dc:	00 08 05 1f	fmadd	s0, s0, s5, s2
1018b92e0:	22 80 04 1f	fmsub	s2, s1, s4, s0
1018b92e4:	04 15 40 2d	ldp	s4, s5, [x8]
1018b92e8:	20 8a 24 1e	fnmul	s0, s17, s4
1018b92ec:	10 1d 41 2d	ldp	s16, s7, [x8, #0x8]
1018b92f0:	c0 00 07 1f	fmadd	s0, s6, s7, s0
1018b92f4:	40 82 05 1f	fmsub	s0, s18, s5, s0
1018b92f8:	43 80 10 1f	fmsub	s3, s2, s16, s0
1018b92fc:	e0 08 31 1e	fmul	s0, s7, s17
1018b9300:	c0 00 04 1f	fmadd	s0, s6, s4, s0
1018b9304:	40 02 10 1f	fmadd	s0, s18, s16, s0
1018b9308:	40 80 05 1f	fmsub	s0, s2, s5, s0
1018b930c:	e1 08 32 1e	fmul	s1, s7, s18
1018b9310:	c1 04 05 1f	fmadd	s1, s6, s5, s1
1018b9314:	41 04 04 1f	fmadd	s1, s2, s4, s1
1018b9318:	21 86 10 1f	fmsub	s1, s17, s16, s1
1018b931c:	e2 08 22 1e	fmul	s2, s7, s2
1018b9320:	c2 08 10 1f	fmadd	s2, s6, s16, s2
1018b9324:	22 0a 05 1f	fmadd	s2, s17, s5, s2
1018b9328:	42 8a 04 1f	fmsub	s2, s18, s4, s2
1018b932c:	c0 03 5f d6	ret
