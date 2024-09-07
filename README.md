# mt-mdb-monitor

This is a simple tool to monitor a MikroTik switch's MDB table and (optionally) send logs to Loki.

## Usage

```shell
mt-mdb-monitor --username admin --password password 192.168.0.4
```

or with Loki logging:
```shell
mt-mdb-monitor --username admin --password password 192.168.0.4 --loki-url http://localhost:3100
```

## Example output

```shell
2024-09-07T17:20:35.148058Z  INFO mt_mdb_monitor: Connected to Mikrotik router on 192.168.0.4:8728
2024-09-07T17:20:35.148326Z  INFO mt_mdb_monitor: Port ether3 joined group ff02::1:ff2d:e836 in VLAN 2
2024-09-07T17:20:35.148534Z  INFO mt_mdb_monitor: Port ether2 joined group ff02::1:ff0a:109 in VLAN 10
2024-09-07T17:20:35.148675Z  INFO mt_mdb_monitor: Port sfp-sfpplus2 joined group ff02::1:ff2e:e714 in VLAN 1
2024-09-07T17:20:35.148816Z  INFO mt_mdb_monitor: Port bridge joined group ff02::6a in VLAN 1
```