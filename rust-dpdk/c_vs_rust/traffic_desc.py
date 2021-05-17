from trex_stl_lib.api import *
class STLS1(object):
    def __init__ (self):
      self.pg_id = 0
      self.fsize = 64

    def create_stream(self, dir, port_id, flows, latency):

        size = self.fsize - 4; # HW will add 4 bytes ethernet CRC
        # UDP packet

        src_mac = "64:4c:36:11:05:a8"
        dst_mac = "64:4c:36:11:09:40"
        src_ip = "10.1.0.1"
        dst_ip = "10.2.0.1"

        base_pkt  = Ether(src=src_mac, dst=dst_mac)/IP(src=src_ip, dst=dst_ip)/UDP()
        pad = max(0, size - len(base_pkt)) * 'x'

        # vm
        vm = STLVM()
        vm.var(name="src_port", min_value=10000, max_value=11000, size=2, op="inc")
        vm.var(name="dst_port", min_value=10000, max_value=11000, size=2, op="inc")

        vm.write(fv_name="src_port", pkt_offset="UDP.sport")
        vm.write(fv_name="dst_port", pkt_offset="UDP.dport")
        vm.fix_chksum()



        pkt = STLPktBuilder(pkt=base_pkt/pad, vm=vm)
        if int(latency):
            return [
                STLStream(packet=pkt, mode=STLTXCont(pps=100), flow_stats=STLFlowStats(pg_id=self.pg_id + 10)) ,
                STLStream(packet=pkt, mode=STLTXCont(pps=100), flow_stats=STLFlowLatencyStats(pg_id=self.pg_id))]
        else:
            return [STLStream(packet=pkt, mode=STLTXCont(pps=100), flow_stats=STLFlowStats(pg_id=self.pg_id + 10))]


    def get_streams(self, fsize=64, direction=0, pg_id=7, flows=1000, latency=0, **kwargs):
        self.fsize = fsize
        self.pg_id = pg_id + kwargs['port_id']
        return self.create_stream(direction,kwargs['port_id'], flows, latency)


# dynamic load - used for trex console or simulator
def register():
    return STLS1()
