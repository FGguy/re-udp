#!/bin/bash
# sets up two network namespaces connected by a veth pair for testing
# only need to run this once (or after a reboot)

ip netns add ns_server
ip netns add ns_client

# virtual ethernet pair, one end per namespace
ip link add veth_server type veth peer name veth_client
ip link set veth_server netns ns_server
ip link set veth_client netns ns_client

ip netns exec ns_server ip addr add 10.0.0.1/24 dev veth_server
ip netns exec ns_server ip link set veth_server up
ip netns exec ns_client ip addr add 10.0.0.2/24 dev veth_client
ip netns exec ns_client ip link set veth_client up

echo "done. to clean up: ip netns del ns_server && ip netns del ns_client"
