#!/bin/bash

# Create namespaces
ip netns add ns_server
ip netns add ns_client

# Create veth pair and move each end into its namespace
ip link add veth_server type veth peer name veth_client
ip link set veth_server netns ns_server
ip link set veth_client netns ns_client

# Assign IPs and bring interfaces up
ip netns exec ns_server ip addr add 10.0.0.1/24 dev veth_server
ip netns exec ns_server ip link set veth_server up
ip netns exec ns_client ip addr add 10.0.0.2/24 dev veth_client
ip netns exec ns_client ip link set veth_client up

echo "Namespaces ready."
echo "To tear down: sudo ip netns del ns_server && sudo ip netns del ns_client"
