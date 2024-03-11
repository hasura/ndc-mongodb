# MongoDB fixtures in the form of docker volume mounting strings
{
  chinook = "${toString ./..}/fixtures/mongodb/chinook:/docker-entrypoint-initdb.d:ro";
}
