TIMES = 50

TIMES.times do |_|
  system("../target/debug/nspt_client #{ARGV.join(" ")}")
end
