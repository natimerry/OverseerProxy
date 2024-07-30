n = int(input())
s = list(map(int, input().split()))
ans = 0
cnt = 0
rng = 0
d = []
for i in range(n):
    rng += 1
    if s[i] < 0:
        cnt += 1
    if i == n - 1 or cnt == 2 and s[i + 1] < 0:
        ans += 1
        cnt = 0
        d.append(rng)
        rng = 0
print(ans)
for i in range(ans):
    print(d[i],end=" ")