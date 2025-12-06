#!/usr/bin/env python3
"""
Test SOAP Services for UPnP MediaServers
"""

from urllib.request import Request, urlopen

# SOAP request pour GetProtocolInfo
GET_PROTOCOL_INFO = """<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:GetProtocolInfo xmlns:u="urn:schemas-upnp-org:service:ConnectionManager:1"/>
  </s:Body>
</s:Envelope>"""

# SOAP request pour Browse
BROWSE_REQUEST = """<?xml version="1.0" encoding="utf-8"?>
<s:Envelope xmlns:s="http://schemas.xmlsoap.org/soap/envelope/" s:encodingStyle="http://schemas.xmlsoap.org/soap/encoding/">
  <s:Body>
    <u:Browse xmlns:u="urn:schemas-upnp-org:service:ContentDirectory:1">
      <ObjectID>0</ObjectID>
      <BrowseFlag>BrowseDirectChildren</BrowseFlag>
      <Filter>*</Filter>
      <StartingIndex>0</StartingIndex>
      <RequestedCount>10</RequestedCount>
      <SortCriteria></SortCriteria>
    </u:Browse>
  </s:Body>
</s:Envelope>"""

SERVERS = {
    "PMO Music": {
        "base": "http://192.168.0.138:8080",
        "content_control": "/device/8b8e9b19-9c65-4d59-b127-b34717658085/service/ContentDirectory/control",
        "conn_control": "/device/8b8e9b19-9c65-4d59-b127-b34717658085/service/ConnectionManager/control",
        "scpd_content": "/device/8b8e9b19-9c65-4d59-b127-b34717658085/service/ContentDirectory/desc.xml",
    },
    "Upmpdcli": {
        "base": "http://192.168.0.200:49152",
        "content_control": "/uuid-c110358f-d885-b44a-d6d3-dca6329ead0d/ctl-urn-schemas-upnp-org-service-ContentDirectory-1",
        "conn_control": "/uuid-c110358f-d885-b44a-d6d3-dca6329ead0d/ctl-urn-schemas-upnp-org-service-ConnectionManager-1",
        "scpd_content": "/uuid-c110358f-d885-b44a-d6d3-dca6329ead0d/urn-schemas-upnp-org-service-ContentDirectory-1.xml",
    },
}

def send_soap_request(url, soap_action, soap_body):
    """Envoie une requête SOAP"""
    try:
        req = Request(
            url,
            data=soap_body.encode('utf-8'),
            headers={
                'Content-Type': 'text/xml; charset="utf-8"',
                'SOAPAction': f'"{soap_action}"',
                'User-Agent': 'PMOMusic/1.0',
            }
        )
        response = urlopen(req, timeout=5)
        return response.read().decode('utf-8'), response.status, dict(response.headers)
    except Exception as e:
        return f"Error: {e}", None, None

def main():
    print("=" * 100)
    print(" 🧪 SOAP Services Testing")
    print("=" * 100)
    print()

    for server_name, server_info in SERVERS.items():
        print("\n" + "=" * 100)
        print(f" 📡 Testing {server_name}")
        print("=" * 100)

        # Test 1: GetProtocolInfo
        print("\n🔌 Test 1: ConnectionManager::GetProtocolInfo")
        print("-" * 100)

        url = server_info["base"] + server_info["conn_control"]
        soap_action = "urn:schemas-upnp-org:service:ConnectionManager:1#GetProtocolInfo"

        print(f"URL: {url}")
        print(f"SOAPAction: {soap_action}")

        response, status, headers = send_soap_request(url, soap_action, GET_PROTOCOL_INFO)

        if status:
            print(f"\n✅ Status: {status}")
            if headers:
                print(f"Content-Type: {headers.get('Content-Type', 'N/A')}")
            print(f"\n📄 Response ({len(response)} bytes):")
            print(response[:1000])
            if len(response) > 1000:
                print(f"... ({len(response) - 1000} more bytes)")
        else:
            print(f"\n❌ Error: {response}")

        # Test 2: Browse
        print("\n\n📁 Test 2: ContentDirectory::Browse")
        print("-" * 100)

        url = server_info["base"] + server_info["content_control"]
        soap_action = "urn:schemas-upnp-org:service:ContentDirectory:1#Browse"

        print(f"URL: {url}")
        print(f"SOAPAction: {soap_action}")

        response, status, headers = send_soap_request(url, soap_action, BROWSE_REQUEST)

        if status:
            print(f"\n✅ Status: {status}")
            if headers:
                print(f"Content-Type: {headers.get('Content-Type', 'N/A')}")
            print(f"\n📄 Response ({len(response)} bytes):")
            print(response[:2000])
            if len(response) > 2000:
                print(f"... ({len(response) - 2000} more bytes)")
        else:
            print(f"\n❌ Error: {response}")

        print("\n")

    # Test 3: Vérifier les SCPD
    print("\n" + "=" * 100)
    print(" 📋 SCPD (Service Control Protocol Description) Verification")
    print("=" * 100)

    for server_name, server_info in SERVERS.items():
        print(f"\n{server_name}:")

        # ContentDirectory SCPD
        scpd_url = server_info["base"] + server_info["scpd_content"]

        print(f"  ContentDirectory SCPD: {scpd_url}")

        try:
            req = Request(scpd_url, headers={'User-Agent': 'PMOMusic/1.0'})
            response = urlopen(req, timeout=3)
            scpd_xml = response.read().decode('utf-8')
            print(f"    ✅ Fetched ({len(scpd_xml)} bytes)")

            # Vérifier les actions
            import re
            actions = re.findall(r'<action>.*?<name>([^<]+)</name>', scpd_xml, re.DOTALL)
            print(f"    Actions: {', '.join(actions)}")
        except Exception as e:
            print(f"    ❌ Error: {e}")

if __name__ == "__main__":
    main()
