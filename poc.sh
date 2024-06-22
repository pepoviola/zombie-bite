#!/usr/bin/env bash
set -euo pipefail

polkadot --chain polkadot --sync warp --database paritydb &
POLKADOT_NODE_PID=$!

set +e
i=0
until curl -s "127.0.0.1:9944"
do
  ((i++))
  if [ "$i" -gt 30 ]
  then
    echo "Waited too long."
    exit 1
  fi
  echo "Waiting for the RPC to be up..."
  sleep 3
done

echo "RPC is up."

until curl -s -H "Content-Type: application/json" -d '{"id":1,"jsonrpc":"2.0","method":"system_health","params":[]}' http://localhost:9944 | jq ".result.isSyncing" | grep -q false
do
  echo "Waiting until warp synced..."
  sleep 10
done

echo "The node is synced."
kill $POLKADOT_NODE_PID
exit 0

