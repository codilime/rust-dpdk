# Rust vs C: l2fwd results comparison

Performance comparison of [l2fwd written in C](https://github.com/DPDK/dpdk/blob/main/examples/l2fwd/main.c) and [l2fwd written in Rust](../l2fwd/src/main.rs).

## Environment description

We've performed tests in two bare-metal environment. Each machine had a NIC with a single 25 Gbps interface. The l2fwd was placed on one bare-metal and the [TRex](https://codilime.com/a-traffic-generator-for-measuring-network-performance/) traffic generator on the other.

<img src="./l2fwd_trex_env.svg" width="100%">

L2fwd BM specification:

```
lshw -class processor
  *-cpu:0
       description: CPU
       product: Intel(R) Xeon(R) Gold 6252 CPU @ 2.10GHz
       vendor: Intel Corp.
       vendor_id: GenuineIntel
       physical id: 13
       bus info: cpu@0
       version: Intel(R) Xeon(R) Gold 6252 CPU @ 2.10GHz
       slot: CPU0
       size: 1544MHz
       capacity: 4GHz
       width: 64 bits
       clock: 100MHz
       capabilities: lm fpu fpu_exception wp vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush dts acpi mmx fxsr sse sse2 ss ht tm pbe syscall nx pdpe1gb rdtscp x86-64 constant_tsc art arch_perfmon pebs bts rep_good nopl xtopology nonstop_tsc aperfmperf eagerfpu pni pclmulqdq dtes64 monitor ds_cpl vmx smx est tm2 ssse3 sdbg fma cx16 xtpr pdcm pcid dca sse4_1 sse4_2 x2apic movbe popcnt tsc_deadline_timer aes xsave avx f16c rdrand lahf_lm abm 3dnowprefetch epb cat_l3 cdp_l3 invpcid_single intel_pt ssbd mba ibrs ibpb stibp ibrs_enhanced tpr_shadow vnmi flexpriority ept vpid fsgsbase tsc_adjust bmi1 hle avx2 smep bmi2 erms invpcid rtm cqm mpx rdt_a avx512f avx512dq rdseed adx smap clflushopt clwb avx512cd avx512bw avx512vl xsaveopt xsavec xgetbv1 cqm_llc cqm_occup_llc cqm_mbm_total cqm_mbm_local dtherm ida arat pln pts hwp hwp_act_window hwp_epp hwp_pkg_req pku ospke avx512_vnni md_clear spec_ctrl intel_stibp flush_l1d arch_capabilities cpufreq
       configuration: cores=24 enabledcores=24 threads=48
  *-cpu:1
       description: CPU
       product: Intel(R) Xeon(R) Gold 6252 CPU @ 2.10GHz
       vendor: Intel Corp.
       vendor_id: GenuineIntel
       physical id: c
       bus info: cpu@1
       version: Intel(R) Xeon(R) Gold 6252 CPU @ 2.10GHz
       slot: CPU1
       size: 1GHz
       capacity: 4GHz
       width: 64 bits
       clock: 100MHz
       capabilities: lm fpu fpu_exception wp vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush dts acpi mmx fxsr sse sse2 ss ht tm pbe syscall nx pdpe1gb rdtscp x86-64 constant_tsc art arch_perfmon pebs bts rep_good nopl xtopology nonstop_tsc aperfmperf eagerfpu pni pclmulqdq dtes64 monitor ds_cpl vmx smx est tm2 ssse3 sdbg fma cx16 xtpr pdcm pcid dca sse4_1 sse4_2 x2apic movbe popcnt tsc_deadline_timer aes xsave avx f16c rdrand lahf_lm abm 3dnowprefetch epb cat_l3 cdp_l3 invpcid_single intel_pt ssbd mba ibrs ibpb stibp ibrs_enhanced tpr_shadow vnmi flexpriority ept vpid fsgsbase tsc_adjust bmi1 hle avx2 smep bmi2 erms invpcid rtm cqm mpx rdt_a avx512f avx512dq rdseed adx smap clflushopt clwb avx512cd avx512bw avx512vl xsaveopt xsavec xgetbv1 cqm_llc cqm_occup_llc cqm_mbm_total cqm_mbm_local dtherm ida arat pln pts hwp hwp_act_window hwp_epp hwp_pkg_req pku ospke avx512_vnni md_clear spec_ctrl intel_stibp flush_l1d arch_capabilities cpufreq
       configuration: cores=24 enabledcores=24 threads=48
```

TRex was generating L2 packets with a single IPv4 and UDP headers with varying UDP source and destination ports. When testing bigger packet sizes, additional data was added at the end of the packet. For more details, refer to the [traffic description](traffic_desc.py).

## RFC2544 results

<p float="left">
	<img src="charts/throughput_bps.svg" width="45%">
	<img src="charts/throughput_pps.svg" width="45%">
</p>
<p float="left">
	<img src="charts/avg_latency.svg" width="45%">
	<img src="charts/jitter.svg" width="45%">
</p>


### C detailed rusults

| size | duration | pkt loss | tx pkts | rx pkts | tx bytes | latency avg | latency max | latency jitter | throughput bps | throughput pps |
|------|----------|----------|---------|---------|----------|-------------|-------------|----------------|----------------|----------------|
| 64 | 30 | 0 | 556930354 | 556788047 | 35643542656 | 22 | 67 | 3 | 8.852169577 | 17.69981534 |
| 68 | 30 | 0 | 559518835 | 559317707 | 38047280780 | 28 | 69 | 9 | 9.449144398 | 17.78023106 |
| 72 | 30 | 0 | 557652994 | 557326823 | 40151015568 | 32 | 74 | 9 | 9.971612585 | 17.71694256 |
| 128 | 30 | 0 | 550632237 | 550354544 | 70480926336 | 46 | 80 | 7 | 17.50412741 | 17.49529978 |
| 256 | 30 | 0 | 336354378 | 336347775 | 86106720768 | 22 | 125 | 3 | 21.38483543 | 10.69220781 |
| 384 | 30 | 0 | 224254737 | 224254737 | 86113819008 | 23 | 114 | 3 | 21.3865983 | 7.1288661 |
| 512 | 30 | 0 | 168205061 | 168205055 | 86120991232 | 24 | 125 | 3 | 21.38837954 | 5.347094695 |
| 640 | 30 | 0 | 134563432 | 134563432 | 86120596480 | 24 | 92 | 5 | 21.3882815 | 4.277656301 |
| 768 | 30 | 0 | 112135659 | 112135659 | 86120186112 | 33 | 115 | 10 | 21.38817959 | 3.564696598 |
| 896 | 30 | 0 | 96115848 | 96115848 | 86119799808 | 52 | 108 | 10 | 21.38808365 | 3.055440521 |
| 1024 | 30 | 0 | 84101003 | 84101003 | 86119427072 | 32 | 113 | 7 | 21.38799108 | 2.673498885 |
| 1280 | 30 | 0 | 67280206 | 67280206 | 86118663680 | 39 | 145 | 7 | 21.38780149 | 2.138780149 |
| 1400 | 30 | 0 | 61513076 | 61513076 | 86118306400 | 32 | 130 | 2 | 21.38771276 | 1.955448023 |
| 1518 | 30 | 0 | 56731188 | 56731188 | 86117943384 | 37 | 83 | 4 | 21.3876226 | 1.803435898 |

### Rust detailed rusults

| size | duration | pkt loss | tx pkts | rx pkts | tx bytes | latency avg | latency max | latency jitter | throughput bps | throughput pps |
|------|----------|----------|---------|---------|----------|-------------|-------------|----------------|----------------|----------------|
| 64 | 30 | 0 | 556930941 | 556745381 | 35643580224 | 39 | 120 | 8 | 8.852178907 | 17.69845902 |
| 68 | 30 | 0 | 559519428 | 559190021 | 38047321104 | 28 | 72 | 10 | 9.449154413 | 17.77617203 |
| 72 | 30 | 0 | 555570789 | 555319885 | 40001096808 | 30 | 77 | 9 | 9.934379858 | 17.65314372 |
| 128 | 30 | 0 | 549460479 | 549122633 | 70330941312 | 41 | 80 | 9 | 17.46687822 | 17.45613839 |
| 256 | 30 | 0 | 336354378 | 336347310 | 86106720768 | 23 | 129 | 3 | 21.38483543 | 10.69219303 |
| 384 | 30 | 0 | 224254737 | 224254618 | 86113819008 | 23 | 121 | 2 | 21.3865983 | 7.128862317 |
| 512 | 30 | 0 | 168205061 | 168205061 | 86120991232 | 21 | 112 | 1 | 21.38837954 | 5.347094886 |
| 640 | 30 | 0 | 134563363 | 134563363 | 86120552320 | 29 | 146 | 5 | 21.38827054 | 4.277654107 |
| 768 | 30 | 0 | 112135659 | 112135659 | 86120186112 | 29 | 92 | 5 | 21.38817959 | 3.564696598 |
| 896 | 30 | 0 | 96115848 | 96115848 | 86119799808 | 27 | 118 | 3 | 21.38808365 | 3.055440521 |
| 1024 | 30 | 0 | 84101030 | 84101030 | 86119454720 | 32 | 119 | 4 | 21.38799795 | 2.673499743 |
| 1280 | 30 | 0 | 67280206 | 67280206 | 86118663680 | 42 | 145 | 4 | 21.38780149 | 2.138780149 |
| 1400 | 30 | 0 | 61513062 | 61513062 | 86118286800 | 49 | 124 | 15 | 21.38770789 | 1.955447578 |
| 1518 | 30 | 0 | 56731200 | 56731200 | 86117961600 | 61 | 115 | 5 | 21.38762712 | 1.803436279 |

## Conclusion

The performance, latency and jitter are almost identical in both cases, which means that Rust is not a bottleneck in this test. L2fwd is a quite simple application so it's not surprising that the results look like this. Probably, the bottleneck in both of these applications lies inside the DPDK internal implementation, which is common for both — C and Rust l2fwds.


