# Reconnect

Reconnect is a web application that allows users to reconnect their network by interacting with the router’s API. It utilizes Rust and Rocket for the backend, and Minijinja for templating.

## Features

- Retrieve current WAN IP address.
- Redial WAN connection.

## Installation

1. Deploy the binary:

   ```shellsession
   $ sudo curl -Lo reconnect --output-dir /usr/local/bin https://github.com/BioniCosmos/reconnect/releases/download/v0.2.0/reconnect-<target>
   $ sudo chmod +x /usr/local/bin/reconnect
   ```

2. Create a systemd service file:

   ```shellsession
   $ sudo vim /etc/systemd/system/reconnect.service
   ```

   Add the following content to the service file:

   ```ini
   [Unit]
   Description=Reconnect
   After=network.target
   
   [Service]
   ExecStart=/usr/local/bin/reconnect
   Restart=on-failure
   Environment=ROCKET_PASSWORD=<your router password>
   
   [Install]
   WantedBy=multi-user.target
   ```

3. Enable and start the service:

   ```shellsession
   $ sudo systemctl --now enable reconnect.service
   ```

4. (optional) Expose the service to the internet with frp:

   - `frpc.toml`:

     ```toml
     serverAddr = "192.0.2.1"
     serverPort = 7000
     
     [[proxies]]
     name = "reconnect"
     type = "tcp"
     localPort = 8000
     remotePort = 80
     ```

   - `frps.toml`:

     ```toml
     bindPort = 7000
     ```

## License

This project is licensed under [the MIT License](./LICENSE).
