-- Add an optional DNS server override for Nmap reverse lookups. Existing
-- scanner settings get an empty list; deployments can set LAN DNS servers
-- such as the gateway when Docker's resolver does not know local PTR records.

UPDATE settings
SET value = jsonb_set(value, '{discovery,dnsServers}', '[]'::jsonb, true),
    updated_at = now()
WHERE key = 'network_scanner'
  AND NOT (value->'discovery' ? 'dnsServers');
