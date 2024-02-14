Some properties to add

## Flags
* `id` field for DSCP
    * https://en.wikipedia.org/wiki/Differentiated_services#Class_Selector
* `blockid` to drop packets from other instances of this program
* `blockcidr` to block range of source IP addresses
* `allowcidr` to allow only range of source IP addresses
* `port` (DONE)
* `interface` (DONE)
    * Receive (DONE)
    * Transmit (DONE)
* `multicast`
    * https://en.wikipedia.org/wiki/Multicast_address
* Overwrite source IP (ehhh)
* Other source rewrites
    * Set source IP to address of outgoing interface
    * Set source port to destination port
* `ttl-id`
* debug
* fork to background
* help

### M-SEARCH
* `msearch`
    * Actions: block, fwd, proxy, dial

