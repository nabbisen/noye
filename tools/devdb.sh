DB_FILE=./noye.sqlite

if [ ! -f "$DB_FILE" ]; then
    echo missing: "$DB_FILE"
    exit 1
fi

sqlite3 "$DB_FILE" "INSERT INTO monitor_config (host, port, protocol) VALUES ('localhost', 443, 'https');"
sqlite3 "$DB_FILE" "INSERT INTO monitor_config (host, port, protocol) VALUES ('localhost', 80, 'http');"
sqlite3 "$DB_FILE" "INSERT INTO monitor_config (host, port, protocol) VALUES ('localhost', 587, 'smtp');"
