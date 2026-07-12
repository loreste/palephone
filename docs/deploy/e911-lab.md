# E911 / emergency calling lab

Pale **fails closed** for emergency numbers when the emergency call plan is not
ready. That is intentional: unsafe routing is worse than a blocked call.

## What “ready” means in code

`GET /v1/emergency/plan?number=911` (authenticated) returns:

| Field | Meaning |
|-------|---------|
| `emergency` | Number matches 911/112/933 or user assignment list |
| `allowed` | May proceed to gateway |
| `reason` | Why blocked or `routable` |
| `location` | Validated emergency location for the caller |
| `gateway` | Selected SIP gateway |

Hard blockers today:

1. No emergency location assignment for the caller  
2. Location not marked `validated`  
3. Enterprise integration `e911` not available  
4. Enterprise integration `pstn_sbc_operator_connect` not available  
5. No matching emergency/SIP gateway route  

SIP INVITE to emergency numbers receives **403** with
`Warning: 399 pale "emergency_not_ready:…"`.

## Lab setup (not carrier certification)

1. **Admin → Emergency**  
   - Create a location with street/city/region/postal/country  
   - Mark it **validated** only after you have a real civic address policy  
   - Assign the location to test users  

2. **Admin → Enterprise integrations**  
   - Enable `e911` with your provider endpoint notes  
   - Enable `pstn_sbc_operator_connect`  

3. **Admin → SIP gateways**  
   - TLS gateway with prefix `+` or empty catch-all  
   - **Probe** TCP reachability  

4. **Dial test (lab only)**  
   - Use carrier test numbers or a lab SBC that **does not** place live 911  
   - Confirm non-ready tenants get 403  
   - Confirm ready tenants get 302 toward the gateway  

## Certification path (operator responsibility)

Pale does **not** certify E911. Operators must:

- Contract a certified E911 / NG911 provider  
- Provision locations and callbacks per jurisdiction  
- Test with the provider’s acceptance suite  
- Keep audit logs of location changes  

## API checks

```bash
# Plan for a user session
curl -sS "$BASE/v1/emergency/plan?number=911" \
  -H "Authorization: Bearer $TOKEN" -H "User-Agent: Pale/admin"

# Validation report includes emergency-related capabilities
curl -sS "$BASE/v1/admin/enterprise-integrations/validation" \
  -H "Authorization: Bearer $TOKEN" -H "User-Agent: Pale/admin"
```

See [pstn-lab.md](pstn-lab.md) for SBC trunk setup.
